import { useId, useState } from "react";
import { presentError } from "../../../daemon/errors";
import { ActionStatus, useAction } from "../../action";
import { basename } from "../../format";
import { useGate, useServices, useStore } from "../../services";
import { Dialog } from "../../Dialog";
import { Button } from "../../theme";
import { LoadableRegion } from "../../widgets";

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
  const formId = useId();
  const [localError, setLocalError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  // The dialog closes on success, so the result shows beside the list; a failure stays in the dialog, where it is read.
  const register = useAction();

  const submit = () => {
    const path = registerPath.trim();
    if (!isAbsolutePath(path)) {
      setLocalError("Enter an absolute repository path.");
      return;
    }
    setLocalError(null);
    void register.run("Registering…", async () => {
      const result = await data.registerProject(path);
      if (!result.ok) {
        const { reason, nextStep } = presentError(result.error);
        throw new Error(`The repository was not registered. ${reason} ${nextStep}`);
      }
      drafts.setRegisterPath("");
      setAdding(false);
      return { kind: "saved", text: "Repository registered." };
    });
  };

  const close = () => {
    setAdding(false);
    setLocalError(null);
    register.clear();
  };

  return (
    <section aria-labelledby={headingId} className="settings-section">
      <div className="settings-head-row">
        <h3 id={headingId}>Your repositories</h3>
        <Button
          onClick={() => {
            register.clear();
            setAdding(true);
          }}
        >
          Add repository
        </Button>
      </div>
      {!adding && <ActionStatus action={register} />}
      {adding && (
        <Dialog
          title="Add a repository"
          onClose={close}
          footer={
            <>
              <Button onClick={close} disabled={register.pending}>
                Cancel
              </Button>
              <Button
                type="submit"
                form={formId}
                variant="primary"
                pending={register.pending}
                disabled={!gate.can("projects") || registerPath.trim() === ""}
              >
                Register repository
              </Button>
            </>
          }
        >
          <form
            id={formId}
            noValidate
            onSubmit={(event) => {
              event.preventDefault();
              submit();
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
              <p className="settings-hint">Enter the full path of a Git repository on this machine.</p>
            </div>
            {!gate.connected && <p className="settings-hint">Registration is unavailable while offline; the draft is kept.</p>}
            {localError && <p role="alert" className="settings-error">{localError}</p>}
            <ActionStatus action={register} />
          </form>
        </Dialog>
      )}
      <LoadableRegion
        what="repositories"
        loadable={projects}
        connected={gate.connected}
        isEmpty={(rows) => rows.length === 0}
        emptyText="None yet. Add the first repository."
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
