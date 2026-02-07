const statusEl = document.getElementById("status");
const form = document.getElementById("start-form");

async function refreshStatus() {
  try {
    const res = await fetch("/api/status");
    if (!res.ok) {
      statusEl.textContent = "No engine running.";
      return;
    }
    const data = await res.json();
    statusEl.textContent = JSON.stringify(data, null, 2);
  } catch (err) {
    statusEl.textContent = `Error: ${err.message}`;
  }
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const formData = new FormData(form);
  const payload = Object.fromEntries(formData.entries());
  payload.cash = Number(payload.cash);
  const res = await fetch("/api/start", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const text = await res.text();
    statusEl.textContent = `Failed to start: ${text}`;
    return;
  }
  const data = await res.json();
  statusEl.textContent = JSON.stringify(data, null, 2);
  await refreshStatus();
});

setInterval(refreshStatus, 5000);
