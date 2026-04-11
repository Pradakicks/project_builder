import { create } from "zustand";
import { devLog } from "../utils/devLog";

export type AppView = "projects" | "editor" | "settings";

interface AppStore {
  view: AppView;
  activeProjectId: string | null;
  setView: (view: AppView) => void;
  openProject: (id: string) => void;
  goToProjects: () => void;
  goToSettings: () => void;
}

export const useAppStore = create<AppStore>((set) => ({
  view: "projects",
  activeProjectId: null,
  setView: (view) => {
    devLog("debug", "Store:App", `View changed to "${view}"`);
    set({ view });
  },
  openProject: (id) => {
    devLog("info", "Store:App", `Opening project ${id}`);
    set({ view: "editor", activeProjectId: id });
  },
  goToProjects: () => {
    devLog("debug", "Store:App", "Navigating to projects");
    set({ view: "projects", activeProjectId: null });
  },
  goToSettings: () => {
    devLog("debug", "Store:App", "Navigating to settings");
    set({ view: "settings" });
  },
}));
