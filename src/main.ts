import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/approved.css";
import { mount } from "svelte";
import App from "./App.svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
