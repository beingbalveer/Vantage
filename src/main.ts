import "./lib/design/styles/tokens.css";
import "./lib/design/styles/base.css";
import "./lib/design/styles/approved.css";
import { mount } from "svelte";
import App from "./App.svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
