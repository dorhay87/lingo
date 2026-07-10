/* @refresh reload */
import { render } from "solid-js/web";
import "../styles/tokens.css";
import "./popup.css";
import { Popup } from "./Popup";

render(() => <Popup />, document.getElementById("root")!);
