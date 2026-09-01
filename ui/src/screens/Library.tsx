export function Library() {
  return (
    <div className="pane">
      <div className="pane-inner">
        <h1 className="screen-title">Library</h1>
        <p className="screen-sub">Everything published or drafted, dense and sortable.</p>

        <table className="table">
          <thead>
            <tr>
              <th>Title</th>
              <th>Date</th>
              <th>Tier</th>
              <th>Words</th>
              <th>Facts</th>
              <th>SEO</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody />
        </table>

        <div className="empty-state" style={{ marginTop: 48 }}>
          <span>
            Nothing published yet. Run a research session or turn on automation.
          </span>
        </div>
      </div>
    </div>
  );
}
