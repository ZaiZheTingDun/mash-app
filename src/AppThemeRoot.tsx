import React from "react";
import { Theme } from "@radix-ui/themes";
import App from "./App";
import { DebugCanvasWindow } from "./components/DebugCanvasWindow";
import { invoke } from "./tauri";
import type { AppTheme } from "./types/theme";

interface AppThemeRootProps {
  isDebugCanvas: boolean;
}

interface StartupMigrationStatus {
  migrated: boolean;
  from: string | null;
  to: string;
}

function getInitialTheme(): AppTheme {
  if (window.matchMedia?.("(prefers-color-scheme: dark)").matches) {
    return "dark";
  }
  return "light";
}

export function AppThemeRoot({ isDebugCanvas }: AppThemeRootProps) {
  const [theme, setTheme] = React.useState<AppTheme>(getInitialTheme);

  React.useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  React.useEffect(() => {
    if (isDebugCanvas) {
      return;
    }
    invoke<StartupMigrationStatus>("run_startup_migration")
      .catch((error: unknown) => {
        console.error("Startup migration failed", error);
      });
  }, [isDebugCanvas]);

  return (
    <Theme
      appearance={theme}
      accentColor="blue"
      grayColor="slate"
      panelBackground="translucent"
      radius="medium"
    >
      {isDebugCanvas ? (
        <DebugCanvasWindow />
      ) : (
        <App theme={theme} onThemeChange={setTheme} />
      )}
    </Theme>
  );
}
