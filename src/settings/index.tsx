/* @refresh reload */
import { render } from "solid-js/web";
import "../styles/tokens.css";
import "./settings.css";
import { Settings } from "./Settings";

render(() => <Settings />, document.getElementById("root")!);
