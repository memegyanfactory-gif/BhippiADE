export function TickerStrip() {
  return (
    <div className="ticker" role="status" aria-label="Live ticker">
      <span className="dot connecting" aria-hidden="true" />
      <span className="ticker-label">live</span>
      <span className="ticker-item">No qualifying stories yet.</span>
    </div>
  );
}
