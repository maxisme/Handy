import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface CopyPromptToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const CopyPromptToggle: React.FC<CopyPromptToggleProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("copy_prompt_enabled") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("copy_prompt_enabled", enabled)}
        isUpdating={isUpdating("copy_prompt_enabled")}
        label={t("settings.advanced.copyPrompt.label")}
        description={t("settings.advanced.copyPrompt.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
