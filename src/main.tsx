import React from "react";
import ReactDOM from "react-dom/client";
import { createHashRouter, RouterProvider } from "react-router";
import { AppShell } from "./components/AppShell";
import "./index.css";
import { AssetsPage } from "./pages/AssetsPage";
import { CalibrationsPage } from "./pages/CalibrationsPage";
import { DashboardPage } from "./pages/DashboardPage";
import { ListsPage } from "./pages/ListsPage";
import { SettingsPage } from "./pages/SettingsPage";

// Hash routing: the app is served from local files, so there is no server to
// resolve deep links. Every route still works after a reload.
const router = createHashRouter([
  {
    path: "/",
    element: <AppShell />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: "assets", element: <AssetsPage /> },
      { path: "calibrations", element: <CalibrationsPage /> },
      { path: "lists", element: <ListsPage /> },
      { path: "settings", element: <SettingsPage /> },
    ],
  },
]);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
