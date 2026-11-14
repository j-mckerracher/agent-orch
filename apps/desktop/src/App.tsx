import { useEffect, type ReactNode } from "react";
import { ConnectionStrip, Crumbs } from "./ui/ConnectionStrip";
import { Coordinator } from "./ui/Coordinator";
import { ProjectDetail } from "./ui/ProjectDetail";
import { ProjectList } from "./ui/ProjectList";
import { FixtureTaskDetail, TaskDetail } from "./ui/TaskDetail";
import { TaskList } from "./ui/TaskList";
import type { GlobalView, Route } from "./ui/router";
import { ServicesProvider, useServices, useStore, type Services } from "./ui/services";
import { AppShell } from "./ui/shell/AppShell";
import { defaultShellSlots } from "./ui/shell/defaultSlots";
import type { ShellSlots } from "./ui/shell/slots";

const GLOBAL_VIEW_TITLES: Record<GlobalView, string> = {
  workspace: "Orchestrator Workspace",
  workstreams: "Workstreams",
  agents: "Agents & Sessions",
  review: "Review & Decisions",
};

/** Renders the content of a global view; it is placed inside the shell's `<main>`. */
export type GlobalViewRenderer = () => ReactNode;

function GlobalViewPlaceholder({ view }: { view: GlobalView }) {
  return (
    <>
      <h1>{GLOBAL_VIEW_TITLES[view]}</h1>
      <p role="status" data-testid={`global-view-${view}`}>
        This view is not part of this build yet. The navigation shell, routing and connection status are available.
      </p>
    </>
  );
}

function Screen({ route, globalViews }: { route: Route; globalViews: Partial<Record<GlobalView, GlobalViewRenderer>> }) {
  const { router, renderReview } = useServices();
  switch (route.name) {
    case "global": {
      const render = globalViews[route.view];
      return <>{render ? render() : <GlobalViewPlaceholder view={route.view} />}</>;
    }
    case "projects":
      return <ProjectList />;
    case "project":
      return <ProjectDetail key={route.projectId} projectId={route.projectId} />;
    case "tasks":
      return <TaskList projectId={route.projectId} filter={route.filter} />;
    case "task":
      return <TaskDetail key={route.taskId} projectId={route.projectId} taskId={route.taskId} />;
    case "fixture-task":
      return <FixtureTaskDetail projectId={route.projectId} id={route.id} />;
    case "coordinator":
      return <Coordinator projectId={route.projectId} />;
    case "review":
      return renderReview ? (
        <>
          <Crumbs items={[
            { label: "Projects", onClick: () => router.navigate({ name: "projects" }) },
            { label: route.projectId, onClick: () => router.navigate({ name: "project", projectId: route.projectId }) },
            { label: "Review" },
          ]} />
          {renderReview({ projectId: route.projectId, taskId: route.taskId, query: route.query })}
        </>
      ) : (
        <main>
          <h1>Review</h1>
          <p role="status">The diff review screen (S6) is delivered by M5-04 and is not part of this build.</p>
          <button type="button" onClick={() => router.navigate({ name: "project", projectId: route.projectId })}>
            Back to project
          </button>
        </main>
      );
  }
}

function ShellHost({
  shellSlots,
  globalViews,
}: {
  shellSlots?: Partial<ShellSlots>;
  globalViews: Partial<Record<GlobalView, GlobalViewRenderer>>;
}) {
  const { router } = useServices();
  const route = useStore(router);
  // Activity is part of the workspace view only; other routes keep the full width.
  const merged: ShellSlots = { ...defaultShellSlots, ...shellSlots };
  const slots: ShellSlots = {
    ...merged,
    activity: route.name === "global" && route.view === "workspace" ? merged.activity : null,
  };
  return (
    <AppShell route={route} connection={<ConnectionStrip />} slots={slots} mainLandmark={route.name === "global"}>
      <Screen route={route} globalViews={globalViews} />
    </AppShell>
  );
}

export interface AppProps {
  services: Services;
  /** Overrides for the shell's slots (see `ui/shell/slots.ts`); omitted slots use honest placeholders. */
  shellSlots?: Partial<ShellSlots>;
  /** Content of each global view; omitted views show a placeholder. */
  globalViews?: Partial<Record<GlobalView, GlobalViewRenderer>>;
}

export function App({ services, shellSlots, globalViews = {} }: AppProps) {
  useEffect(() => {
    if (services.bridgeBound) services.connection.start();
    return () => {
      services.connection.stop();
      services.data.dispose();
    };
  }, [services]);
  return (
    <ServicesProvider services={services}>
      <ShellHost shellSlots={shellSlots} globalViews={globalViews} />
    </ServicesProvider>
  );
}

export default App;
