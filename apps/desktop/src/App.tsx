import { useEffect } from "react";
import { ConnectionStrip } from "./ui/ConnectionStrip";
import { Coordinator } from "./ui/Coordinator";
import { ProjectDetail } from "./ui/ProjectDetail";
import { ProjectList } from "./ui/ProjectList";
import { FixtureTaskDetail, TaskDetail } from "./ui/TaskDetail";
import { TaskList } from "./ui/TaskList";
import { ServicesProvider, useServices, useStore, type Services } from "./ui/services";

function Screen() {
  const { router, renderReview } = useServices();
  const route = useStore(router);
  switch (route.name) {
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
        <>{renderReview({ projectId: route.projectId, taskId: route.taskId })}</>
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

export function App({ services }: { services: Services }) {
  useEffect(() => {
    if (services.bridgeBound) services.connection.start();
    return () => {
      services.connection.stop();
      services.data.dispose();
    };
  }, [services]);
  return (
    <ServicesProvider services={services}>
      <ConnectionStrip />
      <Screen />
    </ServicesProvider>
  );
}

export default App;
