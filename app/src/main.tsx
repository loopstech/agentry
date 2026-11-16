import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/geist-sans/400.css";
import "@fontsource/geist-sans/500.css";
import "@fontsource/geist-sans/600.css";
import "@fontsource/geist-sans/700.css";
import "@fontsource/geist-mono/400.css";
import "./components/components.css";
import { Island } from "./components/Island";
import { mono } from "./themes/mono";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Island theme={mono} />
  </React.StrictMode>,
);
