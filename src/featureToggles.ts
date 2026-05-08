interface FeatureToggleEnv {
  DEV: boolean;
  VITE_FEATURE_SERVANT_ENHANCEMENT?: string;
  VITE_FEATURE_CV_DEBUG?: string;
}

export interface FeatureToggles {
  servantEnhancement: boolean;
  cvDebug: boolean;
}

function envFlag(value: string | undefined, fallback: boolean): boolean {
  if (value == null || value.trim() === "") return fallback;
  const normalized = value.trim().toLowerCase();
  return ["1", "true", "yes", "on", "enabled"].includes(normalized);
}

export function createFeatureToggles(env: FeatureToggleEnv): FeatureToggles {
  return {
    servantEnhancement: envFlag(
      env.VITE_FEATURE_SERVANT_ENHANCEMENT,
      env.DEV
    ),
    cvDebug: envFlag(env.VITE_FEATURE_CV_DEBUG, env.DEV),
  };
}

export const featureToggles = createFeatureToggles(import.meta.env);
