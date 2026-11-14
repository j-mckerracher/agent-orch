/** S1: project list and repository registration. */
import { useState } from "react";
import type { DaemonError } from "../daemon/errors";
import { Crumbs } from "./ConnectionStrip";
import { basename } from "./format";
import { useGate, useServices, useStore } from "./services";
import { ErrorNotice, LoadableRegion } from "./widgets";
import { Button } from "./theme";

function isAbsolutePath(path: string): boolean {
  return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\");
}

export function ProjectList() {
  const { data, router, drafts } = useServices();
  const { projects } = useStore(data);
  const { registerPath } = useStore(drafts);
  const gate = useGate();
  const [error, setError] = useState<DaemonError | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const register = async () => {
    const path = registerPath.trim();
    setError(null);
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
      router.navigate({ name: "project", projectId: result.value.id });
    } else {
      setError(result.error);
    }
  };

  return (
    <main className="ps-page">
      <Crumbs items={[{ label: "Projects" }]} />
      <h1>Projects</h1>
      <form
        className="ps-inline-form"
        onSubmit={(event) => {
          event.preventDefault();
          void register();
        }}
      >
        <label>
          Register repository: absolute path
          <input
            value={registerPath}
            onChange={(event) => drafts.setRegisterPath(event.target.value)}
            placeholder="/path/to/repository"
          />
        </label>
        <Button type="submit" variant="primary" disabled={!gate.can("projects") || busy || registerPath.trim() === ""}>
          Register
        </Button>
        {!gate.connected && (
          <span className="muted ps-inline-full"> Registration is unavailable while offline; the draft is kept.</span>
        )}
        {localError && (
          <p role="alert" className="ps-inline-full">
            {localError}
          </p>
        )}
        {error && <ErrorNotice error={error} action="Registering the repository" />}
      </form>
      <LoadableRegion
        what="projects"
        loadable={projects}
        connected={gate.connected}
        isEmpty={(rows) => rows.length === 0}
        emptyText="No repositories registered."
        onRetry={() => void data.refreshAll()}
      >
        {(rows) => (
          <ul aria-label="Projects" className="ps-list">
            {rows.map((row) => (
              <li key={row.id} className="ps-project-row">
                <strong>{basename(row.id)}</strong> <code>{row.id}</code>{" "}
                <Button onClick={() => router.navigate({ name: "project", projectId: row.id })}>Open</Button>
              </li>
            ))}
          </ul>
        )}
      </LoadableRegion>
    </main>
  );
}
