import React from "react";
import { Theme } from "@radix-ui/themes";
import App from "./App";
import { DebugCanvasWindow } from "./features/debug/DebugCanvasWindow";
import { invoke } from "./tauri";
import type { AppTheme, AppThemePreference } from "./types/theme";
import {
  DEFAULT_MYSTIC_CODE_GENDER,
  normalizeMysticCodeGender,
  type MysticCodeGender,
} from "./types/appUiSettings";

interface AppThemeRootProps {
  isDebugCanvas: boolean;
}

interface StartupMigrationStatus {
  migrated: boolean;
  from: string | null;
  to: string;
}

function getSystemTheme(): AppTheme {
  if (window.matchMedia?.("(prefers-color-scheme: dark)").matches) {
    return "dark";
  }
  return "light";
}

function isThemePreference(value: unknown): value is AppThemePreference {
  return value === "light" || value === "dark" || value === "system";
}

export function AppThemeRoot({ isDebugCanvas }: AppThemeRootProps) {
  const [themePreference, setThemePreference] =
    React.useState<AppThemePreference>("system");
  const [systemTheme, setSystemTheme] = React.useState<AppTheme>(getSystemTheme);
  const [startupReady, setStartupReady] = React.useState(isDebugCanvas);
  const [mysticCodeGender, setMysticCodeGender] = React.useState<MysticCodeGender>(
    DEFAULT_MYSTIC_CODE_GENDER,
  );
  const theme = themePreference === "system" ? systemTheme : themePreference;

  const handleThemePreferenceChange = React.useCallback((nextTheme: AppThemePreference) => {
    setThemePreference(nextTheme);
    invoke("set_app_theme", { theme: nextTheme }).catch((error: unknown) => {
      console.error("Failed to persist app theme", error);
    });
  }, []);

  const handleMysticCodeGenderChange = React.useCallback((value: MysticCodeGender) => {
    setMysticCodeGender(value);
    invoke("set_mystic_code_gender", { value }).catch((error: unknown) => {
      console.error("Failed to persist Mystic Code gender", error);
    });
  }, []);

  React.useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  React.useEffect(() => {
    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!media) return;
    const handleSystemThemeChange = () => {
      setSystemTheme(media.matches ? "dark" : "light");
    };
    media.addEventListener?.("change", handleSystemThemeChange);
    return () => {
      media.removeEventListener?.("change", handleSystemThemeChange);
    };
  }, []);

  React.useEffect(() => {
    invoke<MysticCodeGender>("get_mystic_code_gender")
      .then((value) => setMysticCodeGender(normalizeMysticCodeGender(value)))
      .catch((error: unknown) => {
        console.error("Failed to load Mystic Code gender", error);
      });
  }, []);

  React.useEffect(() => {
    invoke<AppThemePreference | null>("get_app_theme")
      .then((savedTheme) => {
        if (isThemePreference(savedTheme)) {
          setThemePreference(savedTheme);
        }
      })
      .catch((error: unknown) => {
        console.error("Failed to load app theme", error);
      });
  }, []);

  React.useEffect(() => {
    if (isDebugCanvas) {
      return;
    }
    invoke<StartupMigrationStatus>("run_startup_migration")
      .catch((error: unknown) => {
        console.error("Startup migration failed", error);
      })
      .finally(() => setStartupReady(true));
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
        <App
          theme={theme}
          themePreference={themePreference}
          onThemeChange={handleThemePreferenceChange}
          mysticCodeGender={mysticCodeGender}
          onMysticCodeGenderChange={handleMysticCodeGenderChange}
          startupReady={startupReady}
        />
      )}
    </Theme>
  );
}
