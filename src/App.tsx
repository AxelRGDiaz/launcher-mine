import { useState } from "react";
import { ThemeProvider, useLauncherConfig } from "@/theme/ThemeProvider";
import { Sidebar, type ScreenId } from "@/components/Sidebar";
import { Home } from "@/screens/Home";
import { Instances } from "@/screens/Instances";
import { Mods } from "@/screens/Mods";
import { Accounts } from "@/screens/Accounts";
import { Settings } from "@/screens/Settings";
import { About } from "@/screens/About";
import { UpdateBanner } from "@/components/UpdateBanner";

function Shell() {
  const { config, loading } = useLauncherConfig();
  const [screen, setScreen] = useState<ScreenId>("home");

  if (loading || !config) {
    return <div className="flex h-screen items-center justify-center text-sm text-text-muted">Cargando…</div>;
  }

  return (
    <div className="flex h-screen flex-col">
      <UpdateBanner />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar active={screen} onNavigate={setScreen} config={config} />
        <main className="flex-1 overflow-hidden">
          {screen === "home" && <Home onNavigate={setScreen} />}
          {screen === "instances" && <Instances />}
          {screen === "mods" && <Mods />}
          {screen === "accounts" && <Accounts />}
          {screen === "settings" && <Settings />}
          {screen === "about" && <About />}
        </main>
      </div>
    </div>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <Shell />
    </ThemeProvider>
  );
}
