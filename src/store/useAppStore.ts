import { create } from "zustand";

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
  setView: (view) => set({ view }),
  openProject: (id) => set({ view: "editor", activeProjectId: id }),
  goToProjects: () => set({ view: "projects" }),
  goToSettings: () => set({ view: "settings" }),
}));
