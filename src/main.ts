import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { applyTheme, savedTheme } from "./lib/theme";
import { applyTextSize } from "./lib/settings.svelte";

applyTheme(savedTheme());
applyTextSize();

const target = document.getElementById("app");
if (!target) throw new Error("#app element is missing from index.html");

export default mount(App, { target });
