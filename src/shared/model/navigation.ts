import { create } from "zustand";
import type { Section } from "../types";

/// App-level navigation state (active sidebar section).
type NavState = {
  section: Section;
  setSection: (section: Section) => void;
};

export const useNav = create<NavState>((set) => ({
  section: "browsers",
  setSection: (section) => set({ section }),
}));
