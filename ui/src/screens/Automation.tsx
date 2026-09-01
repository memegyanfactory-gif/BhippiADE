export function Automation() {
  return (
    <div className="pane">
      <div className="pane-inner">
        <h1 className="screen-title">Automation</h1>
        <p className="screen-sub">Unattended operation — timer and ticker runs.</p>

        <div className="empty-state">
          <span>
            Nothing is automated yet. The scheduler, review queue and kill switch land with the
            automation sprint.
          </span>
        </div>

        <section className="settings-section" style={{ marginTop: 32 }}>
          <h2 className="settings-heading">Coming in order</h2>
          <table className="table">
            <thead>
              <tr>
                <th>Piece</th>
                <th>What it does</th>
                <th>Sprint</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>Fact gate</td>
                <td>Blocks any post whose claims fail provenance and corroboration</td>
                <td className="num">S4</td>
              </tr>
              <tr>
                <td>Licence gate</td>
                <td>Every image licensed before a post can build</td>
                <td className="num">S6</td>
              </tr>
              <tr>
                <td>Verification gate</td>
                <td>Blocking pre-publish check of the built site</td>
                <td className="num">S8</td>
              </tr>
              <tr>
                <td>Scheduler + ticker triggers</td>
                <td>One wire story ⇒ one session; caps and quiet hours enforced</td>
                <td className="num">S9</td>
              </tr>
              <tr>
                <td>Kill switch</td>
                <td>Stops everything within 3 s</td>
                <td className="num">S9</td>
              </tr>
            </tbody>
          </table>
        </section>
      </div>
    </div>
  );
}
