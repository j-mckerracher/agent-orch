import { useEffect, type ReactNode } from "react";
import { ConnectionStrip } from "./ui/ConnectionStrip";
import { DesktopUpdates } from "./ui/DesktopUpdates";
import { ProjectDetail } from "./ui/ProjectDetail";
import { ProjectList } from "./ui/ProjectList";
import { FixtureTaskDetail, TaskDetail } from "./ui/TaskDetail";
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
  settings: "Settings",
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
  switch (route.name) {
    case "global": {
      const render = globalViews[route.view];
      return <>{render ? render() : <GlobalViewPlaceholder view={route.view} />}</>;
    }
    case "projects":
      return <ProjectList />;
    case "project":
      return <ProjectDetail key={route.projectId} projectId={route.projectId} tab="tasks" />;
    case "tasks":
      return <ProjectDetail key={route.projectId} projectId={route.projectId} tab="tasks" filter={route.filter} />;
    case "task":
      return <TaskDetail key={route.taskId} projectId={route.projectId} taskId={route.taskId} />;
    case "fixture-task":
      return <FixtureTaskDetail projectId={route.projectId} id={route.id} />;
    case "coordinator":
      return <ProjectDetail key={route.projectId} projectId={route.projectId} tab="coordinator" />;
    case "defaults":
      return <ProjectDetail key={route.projectId} projectId={route.projectId} tab="defaults" />;
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
  const slots: ShellSlots = { ...defaultShellSlots, ...shellSlots };
  return (
    <AppShell
      route={route}
      connection={
        <>
          <ConnectionStrip />
          <DesktopUpdates />
        </>
      }
      slots={slots}
      mainLandmark={route.name === "global"}
    >
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
