// Which of the three views is showing.

export type View = "batch" | "editor" | "settings";

export const nav = $state<{ view: View }>({ view: "batch" });
