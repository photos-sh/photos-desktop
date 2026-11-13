import "./app.css";

import { mount } from "svelte";

async function start() {
  // `vite --mode mock`: the UI in a plain browser, with the core faked.
  // This branch is not in a normal build.
  if (import.meta.env.MODE === "mock") {
    const { installMock } = await import("./dev/mock");
    installMock();
  }
  const { default: App } = await import("./App.svelte");
  const target = document.getElementById("app");
  if (target) mount(App, { target });
}

void start();
