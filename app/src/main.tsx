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
import { Welcome } from "./settings/Welcome";

// One bundle, three views: the island, Settings (#settings) and the first-run
// welcome (#welcome).
const page = window.location.hash;
const view = page === "#settings" ? <Settings /> : page === "#welcome" ? <Welcome /> : <Island />;

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode>{view}</React.StrictMode>);
