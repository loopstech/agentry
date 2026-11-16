import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/geist-sans/400.css";
import "@fontsource/geist-sans/500.css";
import "@fontsource/geist-sans/600.css";
import "@fontsource/geist-sans/700.css";
import "@fontsource/geist-mono/400.css";
import "@fontsource/bricolage-grotesque/400.css";
import "@fontsource/bricolage-grotesque/600.css";
import "@fontsource/bricolage-grotesque/800.css";
import "./components/components.css";
import { Island } from "./components/Island";
import { Settings } from "./settings/Settings";

// One bundle, two windows: the island, and Settings (index.html#settings).
const isSettings = window.location.hash === "#settings";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isSettings ? <Settings /> : <Island />}</React.StrictMode>,
);
