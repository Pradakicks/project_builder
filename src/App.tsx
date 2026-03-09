import { AppLayout } from "./components/layout/AppLayout";
import { ProjectsPage } from "./components/projects/ProjectsPage";
import { SettingsPage } from "./components/settings/SettingsPage";
import { ToastContainer } from "./components/ui/ToastContainer";
import { ConfirmDialog } from "./components/ui/ConfirmDialog";
import { useAppStore } from "./store/useAppStore";

function App() {
  const view = useAppStore((s) => s.view);

  return (
    <>
      {view === "projects" && <ProjectsPage />}
      {view === "editor" && <AppLayout />}
      {view === "settings" && <SettingsPage />}
      <ToastContainer />
      <ConfirmDialog />
    </>
  );
}

export default App;
