interface FeatureToggleEnv {
  DEV: boolean;
  VITE_FEATURE_SERVANT_ENHANCEMENT?: string;
  VITE_FEATURE_CRAFT_ESSENCE_ENHANCEMENT?: string;
  VITE_FEATURE_FRIEND_POINT_SUMMON?: string;
  VITE_FEATURE_RANK_UP_QUEST?: string;
  VITE_FEATURE_CV_DEBUG?: string;
  VITE_FEATURE_GRAND_CARD_PRIORITY?: string;
  VITE_FEATURE_TURN_ATTACK_MODES?: string;
  VITE_FEATURE_AUTO_FRIEND_REQUEST?: string;
}

export interface FeatureToggles {
  servantEnhancement: boolean;
  craftEssenceEnhancement: boolean;
  friendPointSummon: boolean;
  rankUpQuest: boolean;
  cvDebug: boolean;
  grandCardPriority: boolean;
  turnAttackModes: boolean;
  settingsDebug: boolean;
  autoFriendRequest: boolean;
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
    craftEssenceEnhancement: envFlag(
      env.VITE_FEATURE_CRAFT_ESSENCE_ENHANCEMENT,
      true
    ),
    friendPointSummon: envFlag(
      env.VITE_FEATURE_FRIEND_POINT_SUMMON,
      env.DEV
    ),
    rankUpQuest: envFlag(env.VITE_FEATURE_RANK_UP_QUEST, env.DEV),
    cvDebug: envFlag(env.VITE_FEATURE_CV_DEBUG, env.DEV),
    grandCardPriority: envFlag(env.VITE_FEATURE_GRAND_CARD_PRIORITY, env.DEV),
    turnAttackModes: envFlag(env.VITE_FEATURE_TURN_ATTACK_MODES, true),
    settingsDebug: env.DEV,
    autoFriendRequest: envFlag(env.VITE_FEATURE_AUTO_FRIEND_REQUEST, false),
  };
}

export const featureToggles = createFeatureToggles(import.meta.env);
