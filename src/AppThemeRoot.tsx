import React from "react";
import { Button, Theme, Text } from "@radix-ui/themes";
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

type StartupMigrationPhase = "running" | "ready" | "error";

function getInitialTheme(): AppTheme {
  if (window.matchMedia?.("(prefers-color-scheme: dark)").matches) {
    return "dark";
  }
  return "light";
}

export function AppThemeRoot({ isDebugCanvas }: AppThemeRootProps) {
  const [theme, setTheme] = React.useState<AppTheme>(getInitialTheme);
  const [migrationPhase, setMigrationPhase] = React.useState<StartupMigrationPhase>(
    isDebugCanvas ? "ready" : "running"
  );
  const [migrationError, setMigrationError] = React.useState<string | null>(null);

  React.useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  React.useEffect(() => {
    if (isDebugCanvas) {
      return;
    }
    let cancelled = false;
    invoke<StartupMigrationStatus>("run_startup_migration")
      .then(() => {
        if (!cancelled) {
          setMigrationPhase("ready");
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setMigrationError(String(error));
          setMigrationPhase("error");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [isDebugCanvas]);

  return (
    <Theme
      appearance={theme}
      accentColor="blue"
      grayColor="slate"
      panelBackground="translucent"
      radius="medium"
    >
      {!isDebugCanvas && migrationPhase !== "ready" ? (
        <div className="startup-migration-page">
          <div className="startup-migration-message">
            <Text size="4" weight="bold">
              正在从 com.mash.app 迁移数据...
            </Text>
            <Text size="2" color="gray">
              该操作只会进行一次。
            </Text>
            {migrationPhase === "error" ? (
              <>
                <Text size="2" color="red">
                  迁移失败：{migrationError}
                </Text>
                <Button size="2" variant="soft" onClick={() => setMigrationPhase("ready")}>
                  继续打开
                </Button>
              </>
            ) : null}
          </div>
        </div>
      ) : isDebugCanvas ? (
        <DebugCanvasWindow />
      ) : (
        <App theme={theme} onThemeChange={setTheme} />
      )}
    </Theme>
  );
}
