import { useEffect, useState } from "react";
import Layout from "./components/Layout";
import { AppView } from "./components/Nav";
import Circle from "./pages/Circle";
import Dashboard from "./pages/Dashboard";
import Ledger from "./pages/Ledger";
import Onboarding from "./pages/Onboarding";
import Schedule from "./pages/Schedule";
import Settings from "./pages/Settings";
import Vault from "./pages/Vault";
import { useAppStore } from "./stores/appStore";

export default function App() {
  const {
    onboardingCompleted,
    loadOnboarding,
    localNode,
    schedule,
    refreshPeers,
    refreshSchedule,
    refreshDashboard,
    refreshPool,
    refreshLedger
  } = useAppStore();

  const [view, setView] = useState<AppView>("dashboard");
  const [circleName] = useState(localStorage.getItem("tokenunion_circle_name") || "midnight union");

  useEffect(() => {
    void loadOnboarding();
  }, [loadOnboarding]);

  useEffect(() => {
    if (!onboardingCompleted) return;
    void refreshPeers();
    void refreshSchedule();
    void refreshDashboard();
    void refreshPool();
    void refreshLedger();
  }, [onboardingCompleted, refreshPeers, refreshSchedule, refreshDashboard, refreshPool, refreshLedger]);

  if (!onboardingCompleted) {
    return <Onboarding />;
  }

  return (
    <Layout
      current={view}
      onChange={setView}
      circleName={circleName}
      displayName={localNode?.display_name || "operator"}
      sharingLabel={schedule?.sharing_override === "paused" ? "paused" : "sharing"}
      availability={localNode?.availability_state || "offline"}
    >
      {view === "dashboard" ? <Dashboard /> : null}
      {view === "circle" ? <Circle /> : null}
      {view === "ledger" ? <Ledger /> : null}
      {view === "vault" ? <Vault /> : null}
      {view === "schedule" ? <Schedule /> : null}
      {view === "settings" ? <Settings /> : null}
    </Layout>
  );
}
