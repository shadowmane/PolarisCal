import React from "react";
import ReactDOM from "react-dom/client";
import { createHashRouter, RouterProvider } from "react-router";
import { AppShell } from "./components/AppShell";
import "./index.css";
import "./lib/theme";
import { AuthProvider } from "./lib/auth";
import { AssetDetailPage } from "./pages/AssetDetailPage";
import { AssetFormPage } from "./pages/AssetFormPage";
import { AssetsPage } from "./pages/AssetsPage";
import { AuditPage } from "./pages/AuditPage";
import { CalibrationsPage } from "./pages/CalibrationsPage";
import { CertificatePage, CertificatesPage } from "./pages/CertificatesPage";
import { DashboardPage } from "./pages/DashboardPage";
import { GenerateCertificatePage } from "./pages/GenerateCertificatePage";
import { BillingPage } from "./pages/BillingPage";
import { InventoryPage } from "./pages/InventoryPage";
import { ListsPage } from "./pages/ListsPage";
import { ReservationsPage } from "./pages/ReservationsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { HistoryReportPage, ReportsPage } from "./pages/ReportsPage";
import { LabelsPage } from "./pages/LabelsPage";
import { UsersPage } from "./pages/UsersPage";

// Hash routing: the app is served from local files, so there is no server to
// resolve deep links. Every route still works after a reload.
const router = createHashRouter([
  {
    path: "/",
    element: <AppShell />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: "assets", element: <AssetsPage /> },
      { path: "assets/new", element: <AssetFormPage key="new" /> },
      { path: "assets/:id", element: <AssetDetailPage /> },
      { path: "assets/:id/edit", element: <AssetFormPage /> },
      { path: "calibrations", element: <CalibrationsPage /> },
      { path: "certificates", element: <CertificatesPage /> },
      { path: "certificates/new", element: <GenerateCertificatePage /> },
      { path: "certificates/:id", element: <CertificatePage /> },
      { path: "inventory", element: <InventoryPage /> },
      { path: "reservations", element: <ReservationsPage /> },
      { path: "lists", element: <ListsPage /> },
      { path: "users", element: <UsersPage /> },
      { path: "audit", element: <AuditPage /> },
      { path: "settings", element: <SettingsPage /> },
      { path: "billing", element: <BillingPage /> },
      { path: "reports", element: <ReportsPage /> },
      { path: "reports/labels", element: <LabelsPage /> },
      { path: "reports/history/:id", element: <HistoryReportPage /> },
    ],
  },
]);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AuthProvider>
      <RouterProvider router={router} />
    </AuthProvider>
  </React.StrictMode>,
);
