import { useId, useState } from "react";
import type { DaemonError } from "../../../daemon/errors";
import { basename } from "../../format";
import { useGate, useServices, useStore } from "../../services";
import { Button } from "../../theme";
import { ErrorNotice, LoadableRegion } from "../../widgets";

function isAbsolutePath(path: string): boolean {
  return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\");
}

export function RepositoriesSection() {
  const { data, drafts } = useServices();
  const { projects } = useStore(data);
  const { registerPath } = useStore(drafts);
  const gate = useGate();
  const inputId = useId();
  const headingId = useId();
  const [error, setError] = useState<DaemonError | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [registered, setRegistered] = useState(false);
  const [busy, setBusy] = useState(false);

  const register = async () => {
    const path = registerPath.trim();
    setError(null);
    setRegistered(false);
    if (!isAbsolutePath(path)) {
      setLocalError("Enter an absolute repository path.");
      return;
    }
    setLocalError(null);
    setBusy(true);
    const result = await data.registerProject(path);
    setBusy(false);
    if (result.ok) {
      drafts.setRegisterPath("");
      setRegistered(true);
    } else {
      setError(result.error);
    }
  };

  return (
    <section aria-labelledby={headingId} className="settings-section">
      <h2 id={headingId}>Repositories</h2>
      <p className="settings-intro">Register a Git repository on this machine by entering its full local path.</p>
      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void register();
        }}
      >
        <div className="settings-row">
          <label htmlFor={inputId}>Repository path</label>
          <input
            id={inputId}
            type="text"
            className="settings-path"
            value={registerPath}
            onChange={(event) => {
              drafts.setRegisterPath(event.target.value);
              setLocalError(null);
            }}
            placeholder="/path/to/repository"
            autoComplete="off"
            spellCheck={false}
          />
          <p className="settings-hint">Enter an absolute path to a Git repository on this machine.</p>
        </div>
        <div className="settings-actions">
          <Button type="submit" variant="primary" disabled={!gate.can("projects") || busy || registerPath.trim() === ""}>
            Register repository
          </Button>
          {!gate.connected && (
            <p className="settings-hint">Registration is unavailable while offline; the draft is kept.</p>
          )}
          {localError && <p role="alert" className="settings-error">{localError}</p>}
          {error && <ErrorNotice error={error} action="Registering the repository" />}
          {registered && <p role="status">Repository registered.</p>}
        </div>
      </form>
      <LoadableRegion
        what="repositories"
        loadable={projects}
        connected={gate.connected}
        isEmpty={(rows) => rows.length === 0}
        emptyText="No repositories registered."
        onRetry={() => void data.refreshAll()}
      >
        {(rows) => (
          <ul aria-label="Registered repositories" className="ps-list">
            {rows.map((row) => (
              <li key={row.id} className="ps-project-row">
                <strong>{basename(row.id)}</strong> <code>{row.id}</code>
              </li>
            ))}
          </ul>
        )}
      </LoadableRegion>
    </section>
  );
}
