import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands, type Availability, type LearnedWord } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface CustomWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const EMPTY_WORDS: string[] = [];

const normalizeCustomWord = (word: string) =>
  word
    .replace(/[<>"']/g, "")
    .replace(/\s+/g, " ")
    .trim();

export const CustomWords: React.FC<CustomWordsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating, refreshSettings } =
      useSettings();
    const [newWord, setNewWord] = useState("");
    const [availability, setAvailability] = useState<Availability | null>(null);
    const [learnedWords, setLearnedWords] = useState<LearnedWord[]>([]);
    const customWordsSetting = getSetting("custom_words");
    const customWords = customWordsSetting ?? EMPTY_WORDS;
    const learnFromCorrections = getSetting("learn_from_corrections") || false;
    const autoLearnFromApps = getSetting("auto_learn_from_apps") || false;
    // Learning from other apps reads the pasted text back through the
    // accessibility API, which only exists on macOS, and surfaces what it
    // learned on the overlay, so it needs the overlay to be visible.
    const osType = useOsType();
    const overlayOff = getSetting("overlay_style") === "none";
    const postProcessEnabled = getSetting("post_process_enabled");
    const postProcessProviderId = getSetting("post_process_provider_id");
    const postProcessModels = getSetting("post_process_models");
    const providers = getSetting("post_process_providers") || [];
    const normalizedWord = normalizeCustomWord(newWord);

    useEffect(() => {
      let cancelled = false;
      commands
        .getLearningAvailability()
        .then((result) => {
          if (!cancelled) setAvailability(result);
        })
        .catch((error) => {
          console.error("Failed to check learning availability:", error);
        });
      return () => {
        cancelled = true;
      };
    }, [postProcessEnabled, postProcessProviderId, postProcessModels]);

    const loadLearnedWords = useCallback(async () => {
      try {
        const result = await commands.getLearnedWords();
        if (result.status === "ok") {
          setLearnedWords(result.data);
        }
      } catch (error) {
        console.error("Failed to load learned words:", error);
      }
    }, []);

    useEffect(() => {
      loadLearnedWords();
    }, [loadLearnedWords, customWordsSetting]);

    const learnedSet = new Set(learnedWords.map((w) => w.meant.toLowerCase()));

    const handleAddWord = () => {
      if (normalizedWord && normalizedWord.length <= 50) {
        if (customWords.includes(normalizedWord)) {
          toast.error(
            t("settings.advanced.customWords.duplicate", {
              word: normalizedWord,
            }),
          );
          return;
        }
        updateSetting("custom_words", [...customWords, normalizedWord]);
        setNewWord("");
      }
    };

    const handleRemoveWord = async (wordToRemove: string) => {
      if (learnedSet.has(wordToRemove.toLowerCase())) {
        try {
          const result = await commands.removeLearnedWord(wordToRemove);
          if (result.status !== "ok") {
            throw new Error(String(result.error));
          }
          await refreshSettings();
          await loadLearnedWords();
        } catch (error) {
          console.error("Failed to remove learned word:", error);
        }
        return;
      }
      updateSetting(
        "custom_words",
        customWords.filter((word) => word !== wordToRemove),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddWord();
      }
    };

    const learningDescription = (() => {
      if (availability === null) {
        return t("settings.advanced.learning.description");
      }
      switch (availability.state) {
        case "ready": {
          const base = t("settings.advanced.learning.description");
          if (availability.local) {
            return `${base} ${t("settings.advanced.learning.local")}`;
          }
          const provider =
            providers.find((p) => p.id === availability.provider_id)?.label ??
            availability.provider_id;
          return `${base} ${t("settings.advanced.learning.cloud", { provider })}`;
        }
        case "post_processing_off":
          return t("settings.advanced.learning.postProcessingOff");
        case "no_model":
          return t("settings.advanced.learning.noModel");
        case "provider_unsupported":
          return t("settings.advanced.learning.providerUnsupported");
        case "apple_intelligence_unavailable":
          return t("settings.advanced.learning.appleUnavailable");
      }
    })();

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.customWords.title")}
          description={t("settings.advanced.customWords.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-40"
              value={newWord}
              onChange={(e) => setNewWord(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t("settings.advanced.customWords.placeholder")}
              variant="compact"
              disabled={isUpdating("custom_words")}
            />
            <Button
              onClick={handleAddWord}
              disabled={
                !normalizedWord ||
                normalizedWord.length > 50 ||
                isUpdating("custom_words")
              }
              variant="primary"
              size="md"
            >
              {t("settings.advanced.customWords.add")}
            </Button>
          </div>
        </SettingContainer>
        <ToggleSwitch
          checked={learnFromCorrections}
          onChange={(enabled) =>
            updateSetting("learn_from_corrections", enabled)
          }
          disabled={availability?.state !== "ready"}
          isUpdating={isUpdating("learn_from_corrections")}
          label={t("settings.advanced.learning.label")}
          description={learningDescription}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {learnFromCorrections && osType === "macos" && (
          <ToggleSwitch
            checked={autoLearnFromApps}
            onChange={(enabled) =>
              updateSetting("auto_learn_from_apps", enabled)
            }
            disabled={overlayOff}
            isUpdating={isUpdating("auto_learn_from_apps")}
            label={t("settings.advanced.learning.appsLabel")}
            description={
              overlayOff
                ? t("settings.advanced.learning.appsNeedsOverlay")
                : t("settings.advanced.learning.appsDescription")
            }
            descriptionMode={descriptionMode}
            grouped={grouped}
          />
        )}
        {customWords.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
          >
            {customWords.map((word) => (
              <Button
                key={word}
                onClick={() => handleRemoveWord(word)}
                disabled={isUpdating("custom_words")}
                variant="secondary"
                size="sm"
                className="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t("settings.advanced.customWords.remove", { word })}
              >
                <span>{word}</span>
                {learnedSet.has(word.toLowerCase()) && (
                  <span className="text-[10px] uppercase tracking-wide text-text/50">
                    {t("settings.advanced.learning.learnedBadge")}
                  </span>
                )}
                <svg
                  className="w-3 h-3"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </Button>
            ))}
          </div>
        )}
      </>
    );
  },
);
