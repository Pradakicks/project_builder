import { Suspense, lazy } from "react";
import { ToastContainer } from "./components/ui/ToastContainer";
import { ConfirmDialog } from "./components/ui/ConfirmDialog";
import { useAppStore } from "./store/useAppStore";

const ProjectsPage = lazy(() =>
  import("./components/projects/ProjectsPage").then((module) => ({
    default: module.ProjectsPage,
  })),
);
const AppLayout = lazy(() =>
  import("./components/layout/AppLayout").then((module) => ({
    default: module.AppLayout,
  })),
);
const SettingsPage = lazy(() =>
  import("./components/settings/SettingsPage").then((module) => ({
    default: module.SettingsPage,
  })),
);
const DevDiagnosticsPanel = import.meta.env.DEV
  ? lazy(() =>
      import("./components/debug/DevDiagnosticsPanel").then((module) => ({
        default: module.DevDiagnosticsPanel,
      })),
    )
  : null;

function FullScreenLoader({ label }: { label: string }) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-gray-950 text-gray-400">
      <div className="flex items-center gap-2">
        <svg
          className="h-5 w-5 animate-spin"
          viewBox="0 0 24 24"
          fill="none"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          />
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
          />
        </svg>
        <p>{label}</p>
      </div>
    </div>
  );
}

function App() {
  const view = useAppStore((s) => s.view);

  return (
    <>
      <Suspense fallback={<FullScreenLoader label="Loading workspace..." />}>
        {view === "projects" && <ProjectsPage />}
        {view === "editor" && <AppLayout />}
        {view === "settings" && <SettingsPage />}
      </Suspense>
      <ToastContainer />
      <ConfirmDialog />
      {DevDiagnosticsPanel ? (
        <Suspense fallback={null}>
          <DevDiagnosticsPanel />
        </Suspense>
      ) : null}
    </>
  );
}

export default App;
