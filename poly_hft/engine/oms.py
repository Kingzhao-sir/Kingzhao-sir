"""
OMS - Order Management System (订单管理系统)
核心模块：订单状态机、执行引擎、风控检查
设计原则：
1. 状态机驱动，防止并发状态不一致
2. 事前风控拦截（价格偏离、仓位限制、频率限制）
3. 支持模拟盘和实盘切换
"""

import time
import uuid
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Dict, List, Optional, Callable, Any
from collections import deque
import threading


class Side(Enum):
    """订单方向"""
    BUY = "BUY"   # 预测 Yes (价格上涨)
    SELL = "SELL" # 预测 No (价格下跌)


class OrderType(Enum):
    """订单类型"""
    LIMIT = "LIMIT"
    MARKET = "MARKET"


class TimeInForce(Enum):
    """订单有效期策略"""
    GTC = "GTC"           # Good Till Cancel
    IOC = "IOC"           # Immediate Or Cancel
    POST_ONLY = "POST"    # Maker only


class OrderStatus(Enum):
    """订单状态机"""
    PENDING_NEW = auto()      # 已创建，等待发送
    NEW = auto()              # 交易所已接收
    PARTIALLY_FILLED = auto() # 部分成交
    FILLED = auto()           # 完全成交
    CANCELED = auto()         # 已取消
    REJECTED = auto()         # 被拒绝
    EXPIRED = auto()          # 过期 (周期结束)


@dataclass
class Order:
    """
    订单数据结构
    紧凑设计，避免不必要的嵌套对象
    """
    id: str
    market_id: str
    side: Side
    order_type: OrderType
    time_in_force: TimeInForce
    price: float          # 单价 (0.0 - 1.0)
    quantity: int         # 数量 (份额)
    status: OrderStatus = OrderStatus.PENDING_NEW
    filled_quantity: int = 0
    average_fill_price: float = 0.0
    created_at: float = field(default_factory=time.time)
    updated_at: float = field(default_factory=time.time)
    signal_trace_id: str = ""
    
    @classmethod
    def create(cls, market_id: str, side: Side, price: float, quantity: int,
               time_in_force: TimeInForce = TimeInForce.GTC,
               signal_trace_id: str = "") -> "Order":
        """工厂方法创建新订单"""
        order_id = str(uuid.uuid4())
        return cls(
            id=order_id,
            market_id=market_id,
            side=side,
            order_type=OrderType.LIMIT,
            time_in_force=time_in_force,
            price=price,
            quantity=quantity,
            signal_trace_id=signal_trace_id,
        )
    
    @property
    def is_active(self) -> bool:
        """检查订单是否活跃"""
        return self.status in {
            OrderStatus.PENDING_NEW,
            OrderStatus.NEW,
            OrderStatus.PARTIALLY_FILLED
        }
    
    @property
    def remaining_qty(self) -> int:
        """剩余未成交数量"""
        return max(0, self.quantity - self.filled_quantity)
    
    def update_timestamp(self):
        """更新时间戳"""
        self.updated_at = time.time()


@dataclass
class ExecutionReport:
    """成交回报"""
    order_id: str
    status: OrderStatus
    last_fill_qty: int = 0
    last_fill_price: float = 0.0
    cumulative_filled_qty: int = 0
    cumulative_avg_price: float = 0.0
    timestamp: float = field(default_factory=time.time)
    message: Optional[str] = None


class RiskEngine:
    """
    风控引擎
    三级风控：事前、事中、事后
    """
    
    def __init__(self, config: Dict[str, Any]):
        self.config = config
        # 事前风控配置
        self.max_order_value = config.get("max_order_value", 1000.0)  # 单笔最大金额
        self.max_daily_loss = config.get("max_daily_loss", 5000.0)    # 当日最大亏损
        self.price_deviation_threshold = config.get("price_deviation_threshold", 0.05)  # 价格偏离阈值
        self.rate_limit_per_second = config.get("rate_limit_per_second", 10)  # 每秒下单频率限制
        
        # 状态跟踪
        self.daily_pnl = 0.0
        self.order_timestamps: deque = deque(maxlen=100)
        self._lock = threading.Lock()
    
    def pre_trade_check(self, order: Order, current_market_price: float) -> tuple[bool, Optional[str]]:
        """
        事前风控检查
        返回: (是否通过, 错误信息)
        """
        with self._lock:
            # 1. 检查订单金额
            order_value = order.price * order.quantity
            if order_value > self.max_order_value:
                return False, f"订单金额 {order_value} 超过限制 {self.max_order_value}"
            
            # 2. 检查当日亏损
            if self.daily_pnl < -self.max_daily_loss:
                return False, f"当日亏损 {abs(self.daily_pnl)} 超过限制 {self.max_daily_loss}"
            
            # 3. 检查价格偏离 (防止乌龙指)
            if current_market_price > 0:
                deviation = abs(order.price - current_market_price) / current_market_price
                if deviation > self.price_deviation_threshold:
                    return False, f"价格偏离 {deviation:.2%} 超过阈值 {self.price_deviation_threshold:.2%}"
            
            # 4. 检查下单频率 (令牌桶简化版)
            now = time.time()
            recent_orders = [t for t in self.order_timestamps if now - t < 1.0]
            if len(recent_orders) >= self.rate_limit_per_second:
                return False, f"下单频率超过限制 {self.rate_limit_per_second}/秒"
            
            return True, None
    
    def record_order_submission(self):
        """记录订单提交时间用于频率限制"""
        self.order_timestamps.append(time.time())
    
    def update_pnl(self, pnl: float):
        """更新当日盈亏"""
        with self._lock:
            self.daily_pnl += pnl


class OMS:
    """
    Order Management System - 订单管理系统核心
    功能:
    1. 订单生命周期管理 (状态机)
    2. 与交易所 API 对接 (模拟/实盘)
    3. 风控集成
    4. 成交回报处理
    """
    
    def __init__(self, risk_config: Dict[str, Any], mode: str = "simulation"):
        """
        Args:
            risk_config: 风控配置字典
            mode: "simulation" (模拟盘) 或 "live" (实盘)
        """
        self.mode = mode
        self.risk_engine = RiskEngine(risk_config)
        
        # 订单存储 (内存中，生产环境可持久化到 Redis/DB)
        self.orders: Dict[str, Order] = {}
        self.orders_by_market: Dict[str, List[str]] = {}  # market_id -> [order_ids]
        
        # 回调函数
        self.on_order_update: Optional[Callable[[ExecutionReport], None]] = None
        
        # 锁保护并发访问
        self._lock = threading.RLock()
        
        # 统计信息
        self.stats = {
            "total_submitted": 0,
            "total_filled": 0,
            "total_canceled": 0,
            "total_rejected": 0,
        }
    
    def set_order_update_callback(self, callback: Callable[[ExecutionReport], None]):
        """设置订单更新回调"""
        self.on_order_update = callback
    
    def submit_order(self, order: Order, current_market_price: float) -> tuple[bool, Optional[str]]:
        """
        提交订单
        流程: 风控检查 -> 状态转换 -> 发送交易所
        """
        with self._lock:
            # 1. 事前风控检查
            passed, error_msg = self.risk_engine.pre_trade_check(order, current_market_price)
            if not passed:
                order.status = OrderStatus.REJECTED
                self.orders[order.id] = order
                self.stats["total_rejected"] += 1
                
                report = ExecutionReport(
                    order_id=order.id,
                    status=OrderStatus.REJECTED,
                    message=error_msg
                )
                if self.on_order_update:
                    self.on_order_update(report)
                
                return False, error_msg
            
            # 2. 记录风控提交
            self.risk_engine.record_order_submission()
            
            # 3. 存储订单并转换状态
            order.status = OrderStatus.NEW
            order.update_timestamp()
            self.orders[order.id] = order
            
            if order.market_id not in self.orders_by_market:
                self.orders_by_market[order.market_id] = []
            self.orders_by_market[order.market_id].append(order.id)
            
            self.stats["total_submitted"] += 1
            
            # 4. 发送交易所 (模拟或实盘)
            if self.mode == "simulation":
                self._simulate_exchange_response(order)
            else:
                self._send_to_live_exchange(order)
            
            return True, None
    
    def _simulate_exchange_response(self, order: Order):
        """模拟交易所响应 (用于回测和模拟盘)"""
        # 简单模拟：90% 概率接受订单
        import random
        
        if random.random() < 0.9:
            # 模拟部分或全部成交
            fill_ratio = min(1.0, random.random() + 0.2)  # 20%-120% 填充率
            fill_qty = int(order.quantity * fill_ratio)
            fill_price = order.price * (1 + random.uniform(-0.01, 0.01))  # ±1% 滑点
            
            self._handle_fill(order, fill_qty, fill_price)
        else:
            # 模拟拒绝
            order.status = OrderStatus.REJECTED
            report = ExecutionReport(
                order_id=order.id,
                status=OrderStatus.REJECTED,
                message="Simulated rejection"
            )
            if self.on_order_update:
                self.on_order_update(report)
    
    def _send_to_live_exchange(self, order: Order):
        """发送到真实交易所 (待实现 Polymarket API 对接)"""
        # TODO: 实现 Polymarket CLOB API 调用
        # 这里应该调用 Polymarket 的下单接口
        print(f"[LIVE MODE] Sending order to Polymarket: {order.id}")
        pass
    
    def _handle_fill(self, order: Order, fill_qty: int, fill_price: float):
        """处理成交流程"""
        with self._lock:
            old_status = order.status
            
            # 处理 fill_qty 为 0 的情况 (可能是部分成交但数量为 0)
            if fill_qty <= 0:
                return
            
            # 更新成交信息
            prev_filled = order.filled_quantity
            order.filled_quantity += fill_qty
            order.average_fill_price = (
                (order.average_fill_price * prev_filled + fill_price * fill_qty) 
                / order.filled_quantity
            )
            
            # 状态转换
            if order.filled_quantity >= order.quantity:
                order.status = OrderStatus.FILLED
                self.stats["total_filled"] += 1
            else:
                order.status = OrderStatus.PARTIALLY_FILLED
            
            order.update_timestamp()
            
            # 发送成交回报
            report = ExecutionReport(
                order_id=order.id,
                status=order.status,
                last_fill_qty=fill_qty,
                last_fill_price=fill_price,
                cumulative_filled_qty=order.filled_quantity,
                cumulative_avg_price=order.average_fill_price,
            )
            
            if self.on_order_update:
                self.on_order_update(report)
    
    def cancel_order(self, order_id: str) -> bool:
        """取消订单"""
        with self._lock:
            if order_id not in self.orders:
                return False
            
            order = self.orders[order_id]
            if not order.is_active:
                return False
            
            order.status = OrderStatus.CANCELED
            order.update_timestamp()
            self.stats["total_canceled"] += 1
            
            report = ExecutionReport(
                order_id=order_id,
                status=OrderStatus.CANCELED,
            )
            
            if self.on_order_update:
                self.on_order_update(report)
            
            return True
    
    def cancel_all_orders(self, market_id: Optional[str] = None):
        """取消所有订单或指定市场的所有订单"""
        with self._lock:
            if market_id:
                order_ids = self.orders_by_market.get(market_id, [])
            else:
                order_ids = list(self.orders.keys())
            
            for order_id in order_ids:
                if order_id in self.orders:
                    order = self.orders[order_id]
                    if order.is_active:
                        self.cancel_order(order_id)
    
    def get_order(self, order_id: str) -> Optional[Order]:
        """获取订单详情"""
        return self.orders.get(order_id)
    
    def get_active_orders(self, market_id: Optional[str] = None) -> List[Order]:
        """获取所有活跃订单"""
        with self._lock:
            if market_id:
                order_ids = self.orders_by_market.get(market_id, [])
            else:
                order_ids = list(self.orders.keys())
            
            return [
                self.orders[oid] 
                for oid in order_ids 
                if oid in self.orders and self.orders[oid].is_active
            ]
    
    def get_stats(self) -> Dict[str, Any]:
        """获取统计信息"""
        return self.stats.copy()
    
    def expire_orders_for_market(self, market_id: str):
        """市场到期时，将所有该市场的订单标记为过期"""
        with self._lock:
            order_ids = self.orders_by_market.get(market_id, [])
            for order_id in order_ids:
                if order_id in self.orders:
                    order = self.orders[order_id]
                    if order.is_active:
                        order.status = OrderStatus.EXPIRED
                        order.update_timestamp()
