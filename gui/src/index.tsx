/* @refresh reload */
import { render } from "solid-js/web";
import "./styles/fonts.css";
import "./styles/organic.css";
import "./styles/tokens.css";
import "./styles/gui-tokens.css";
import "./styles/app.css";
import { App } from "./App";

render(() => <App />, document.getElementById("root")!);
