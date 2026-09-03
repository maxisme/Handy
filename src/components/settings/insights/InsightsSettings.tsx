import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Bot,
  CodeXml,
  FileText,
  Globe,
  Mail,
  MessageCircle,
  MessageSquareText,
  TrendingDown,
  TrendingUp,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import {
  commands,
  events,
  type CategoryUsage,
  type DayActivity,
  type InsightsStats,
  type UsageCategory,
} from "@/bindings";

/** Typing speed the words-per-minute gauge is compared against. */
const TYPING_WPM = 40;
/** Fastest pace that fills the gauge completely. */
const GAUGE_MAX_WPM = 200;
/** Weeks of history drawn in the streak calendar. */
const CALENDAR_WEEKS = 26;

const CATEGORY_ICONS: Record<UsageCategory, LucideIcon> = {
  aiPrompts: Bot,
  workMessages: MessageSquareText,
  personalMessages: MessageCircle,
  emails: Mail,
  documents: FileText,
  code: CodeXml,
  other: Globe,
};

const Card: React.FC<{ className?: string; children: React.ReactNode }> = ({
  className = "",
  children,
}) => (
  <div
    className={`bg-background border border-mid-gray/20 rounded-lg p-4 ${className}`}
  >
    {children}
  </div>
);

const CardLabel: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="text-[10px] font-medium text-mid-gray uppercase tracking-wider">
    {children}
  </div>
);

const BigNumber: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="text-3xl font-semibold tracking-tight tabular-nums">
    {children}
  </div>
);

/** Half-circle gauge filled in proportion to `value / max`. */
const WpmGauge: React.FC<{ value: number | null; max: number }> = ({
  value,
  max,
}) => {
  const radius = 42;
  const circumference = Math.PI * radius;
  const fraction = value === null ? 0 : Math.min(value / max, 1);
  return (
    <svg
      viewBox="0 0 100 56"
      className="w-full max-w-[140px] mx-auto"
      aria-hidden="true"
    >
      <path
        d="M 8 50 A 42 42 0 0 1 92 50"
        fill="none"
        strokeWidth="9"
        strokeLinecap="round"
        className="stroke-mid-gray/20"
      />
      <path
        d="M 8 50 A 42 42 0 0 1 92 50"
        fill="none"
        strokeWidth="9"
        strokeLinecap="round"
        className="stroke-logo-primary transition-[stroke-dashoffset] duration-700 ease-out"
        strokeDasharray={circumference}
        strokeDashoffset={circumference * (1 - fraction)}
      />
    </svg>
  );
};

interface CalendarCell {
  key: string;
  date: Date | null;
  activity: DayActivity | null;
  level: 0 | 1 | 2 | 3 | 4;
  inCurrentStreak: boolean;
  isToday: boolean;
}

const LEVEL_CLASSES: Record<CalendarCell["level"], string> = {
  0: "bg-mid-gray/15",
  1: "bg-logo-primary/30",
  2: "bg-logo-primary/55",
  3: "bg-logo-primary/80",
  4: "bg-logo-primary",
};

const isoDate = (date: Date): string => {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
};

const addDays = (date: Date, days: number): Date => {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
};

/**
 * Lays the last `CALENDAR_WEEKS` weeks out as columns of seven days starting
 * on Sunday. Days after today are empty cells so the grid stays rectangular.
 */
const buildCalendar = (
  stats: InsightsStats,
  today: Date,
): { weeks: CalendarCell[][]; monthStarts: Map<number, Date> } => {
  const byDate = new Map(stats.activity.map((day) => [day.date, day]));
  const maxWords = stats.activity.reduce(
    (max, day) => Math.max(max, day.words),
    0,
  );
  const level = (words: number): CalendarCell["level"] => {
    if (words <= 0 || maxWords <= 0) return 0;
    const ratio = words / maxWords;
    if (ratio > 0.75) return 4;
    if (ratio > 0.5) return 3;
    if (ratio > 0.25) return 2;
    return 1;
  };

  const streakDates = new Set<string>();
  if (stats.current_streak > 0) {
    const first = stats.active_today ? today : addDays(today, -1);
    for (let i = 0; i < stats.current_streak; i++) {
      streakDates.add(isoDate(addDays(first, -i)));
    }
  }

  const lastColumnStart = addDays(today, -today.getDay());
  const gridStart = addDays(lastColumnStart, -7 * (CALENDAR_WEEKS - 1));
  const todayKey = isoDate(today);
  const weeks: CalendarCell[][] = [];
  const monthStarts = new Map<number, Date>();
  let lastMonth = -1;

  for (let w = 0; w < CALENDAR_WEEKS; w++) {
    const week: CalendarCell[] = [];
    for (let d = 0; d < 7; d++) {
      const date = addDays(gridStart, w * 7 + d);
      const key = isoDate(date);
      if (date > today) {
        week.push({
          key,
          date: null,
          activity: null,
          level: 0,
          inCurrentStreak: false,
          isToday: false,
        });
        continue;
      }
      if (d === 0 && date.getMonth() !== lastMonth) {
        monthStarts.set(w, date);
        lastMonth = date.getMonth();
      }
      const activity = byDate.get(key) ?? null;
      week.push({
        key,
        date,
        activity,
        level: level(activity?.words ?? 0),
        inCurrentStreak: streakDates.has(key),
        isToday: key === todayKey,
      });
    }
    weeks.push(week);
  }
  return { weeks, monthStarts };
};

export const InsightsSettings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [stats, setStats] = useState<InsightsStats | null>(null);
  const [failed, setFailed] = useState(false);

  const load = useCallback(async () => {
    try {
      const result = await commands.getInsights();
      if (result.status === "ok") {
        setStats(result.data);
        setFailed(false);
      } else {
        console.error("Failed to load insights:", result.error);
        setFailed(true);
      }
    } catch (error) {
      console.error("Failed to load insights:", error);
      setFailed(true);
    }
  }, []);

  useEffect(() => {
    load();
    const unlisten = events.historyUpdatePayload.listen(() => {
      load();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [load]);

  const numberFormat = useMemo(
    () => new Intl.NumberFormat(i18n.language),
    [i18n.language],
  );
  const monthFormat = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { month: "short" }),
    [i18n.language],
  );
  const weekdayFormat = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { weekday: "short" }),
    [i18n.language],
  );
  const dateFormat = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        month: "long",
        day: "numeric",
        year: "numeric",
      }),
    [i18n.language],
  );

  const today = useMemo(() => {
    const now = new Date();
    now.setHours(0, 0, 0, 0);
    return now;
  }, [stats]);

  const calendar = useMemo(
    () => (stats ? buildCalendar(stats, today) : null),
    [stats, today],
  );

  if (failed) {
    return (
      <div className="max-w-3xl w-full mx-auto">
        <p className="text-sm text-error">{t("settings.insights.error")}</p>
      </div>
    );
  }

  if (!stats || !calendar) {
    return (
      <div className="max-w-3xl w-full mx-auto">
        <p className="text-sm text-mid-gray">
          {t("settings.insights.loading")}
        </p>
      </div>
    );
  }

  const wpm =
    stats.words_per_minute === null ? null : Math.round(stats.words_per_minute);
  const totalFixes = stats.dictionary_fixes + stats.post_process_fixes;
  const monthDelta =
    stats.words_previous_month > 0
      ? Math.round(
          ((stats.words_this_month - stats.words_previous_month) /
            stats.words_previous_month) *
            100,
        )
      : null;

  const categoryTotal = stats.categories.reduce(
    (sum, c) => sum + c.dictations,
    0,
  );
  const categories: CategoryUsage[] = [...stats.categories].sort(
    (a, b) => b.dictations - a.dictations,
  );

  // Weekday labels down the left edge, drawn on Mon/Wed/Fri like the grid.
  const weekdayLabels = [1, 3, 5].map((offset) => ({
    row: offset,
    label: weekdayFormat.format(
      addDays(calendar.weeks[0][0].date ?? today, offset),
    ),
  }));

  return (
    <div className="max-w-3xl w-full mx-auto space-y-4">
      {stats.total_dictations === 0 && (
        <p className="text-sm text-mid-gray px-1">
          {t("settings.insights.empty")}
        </p>
      )}

      <div className="grid grid-cols-3 gap-3">
        <Card className="flex flex-col">
          <BigNumber>
            {wpm === null ? (
              <span className="text-mid-gray">{"–"}</span>
            ) : (
              numberFormat.format(wpm)
            )}
          </BigNumber>
          <CardLabel>{t("settings.insights.wordsPerMinute")}</CardLabel>
          <div className="mt-3">
            <WpmGauge value={wpm} max={GAUGE_MAX_WPM} />
            <div className="mt-1 text-center text-[11px] text-mid-gray leading-tight">
              {wpm === null
                ? t("settings.insights.wpmEmpty")
                : t("settings.insights.typingBaseline", {
                    multiplier: (wpm / TYPING_WPM).toFixed(1),
                    typing: TYPING_WPM,
                  })}
            </div>
          </div>
        </Card>

        <Card>
          <BigNumber>{numberFormat.format(totalFixes)}</BigNumber>
          <CardLabel>{t("settings.insights.fixesTitle")}</CardLabel>
          <div className="mt-3 pt-3 border-t border-mid-gray/20 space-y-1.5 text-sm">
            <div title={t("settings.insights.dictionaryFixesHint")}>
              <span className="font-medium tabular-nums">
                {numberFormat.format(stats.dictionary_fixes)}
              </span>{" "}
              <span className="text-text/70">
                {t("settings.insights.dictionaryFixesLabel")}
              </span>
            </div>
            <div title={t("settings.insights.postProcessFixesHint")}>
              <span className="font-medium tabular-nums">
                {numberFormat.format(stats.post_process_fixes)}
              </span>{" "}
              <span className="text-text/70">
                {t("settings.insights.postProcessFixesLabel")}
              </span>
            </div>
          </div>
        </Card>

        <Card>
          <BigNumber>{numberFormat.format(stats.total_words)}</BigNumber>
          <CardLabel>{t("settings.insights.totalWords")}</CardLabel>
          <div className="mt-3 pt-3 border-t border-mid-gray/20 text-sm">
            {monthDelta === null ? (
              <span className="text-text/70">
                {t("settings.insights.monthNew", {
                  total: numberFormat.format(stats.words_this_month),
                })}
              </span>
            ) : (
              <span className="inline-flex items-center gap-1 rounded-md bg-mid-gray/10 px-2 py-0.5 text-xs font-medium">
                {monthDelta >= 0 ? (
                  <TrendingUp size={12} className="text-logo-primary" />
                ) : (
                  <TrendingDown size={12} className="text-mid-gray" />
                )}
                {t("settings.insights.monthChange", {
                  percent: `${monthDelta >= 0 ? "+" : ""}${monthDelta}`,
                })}
              </span>
            )}
            <div className="text-xs text-mid-gray mt-1.5">
              {t("settings.insights.dictationsCount", {
                total: numberFormat.format(stats.total_dictations),
              })}
            </div>
          </div>
        </Card>
      </div>

      <Card>
        <div className="flex items-baseline justify-between gap-4 mb-4">
          <h2 className="text-lg font-semibold">
            {t("settings.insights.usageTitle")}
          </h2>
          <CardLabel>
            {t("settings.insights.totalApps")}
            {" | "}
            {numberFormat.format(stats.total_apps)}
          </CardLabel>
        </div>
        {categoryTotal === 0 ? (
          <p className="text-sm text-text/70">
            {t("settings.insights.noAppData", {
              total: numberFormat.format(stats.unattributed),
            })}
          </p>
        ) : (
          <div className="space-y-2.5">
            {categories.map((usage) => {
              const Icon = CATEGORY_ICONS[usage.category];
              const percent =
                categoryTotal > 0
                  ? Math.round((usage.dictations / categoryTotal) * 100)
                  : 0;
              return (
                <div key={usage.category} className="flex items-center gap-3">
                  <Icon size={18} className="text-text/60 shrink-0" />
                  <div className="flex-1 flex items-center gap-3 min-w-0">
                    {/* The bar fills a track of fixed width rather than the row
                      itself, so a category at 100% cannot squeeze its own
                      label out of view. */}
                    <div className="w-1/2 shrink-0">
                      <div
                        className="h-7 rounded-md bg-logo-primary flex items-center justify-center text-xs font-semibold text-black/75 transition-[width] duration-500 ease-out"
                        style={{
                          width: `max(2.75rem, ${percent}%)`,
                          opacity: usage.dictations === 0 ? 0.35 : 1,
                        }}
                      >
                        {`${percent}%`}
                      </div>
                    </div>
                    <div className="text-xs font-medium uppercase tracking-wider truncate">
                      <span className="tabular-nums">
                        {numberFormat.format(usage.dictations)}
                      </span>{" "}
                      {t(`settings.insights.category.${usage.category}`)}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
        {categoryTotal > 0 && stats.unattributed > 0 && (
          <p className="mt-3 text-[11px] text-mid-gray">
            {t("settings.insights.unattributed", {
              total: numberFormat.format(stats.unattributed),
            })}
          </p>
        )}
        {stats.top_apps.length > 0 && (
          <div className="mt-4 pt-3 border-t border-mid-gray/20">
            <CardLabel>{t("settings.insights.topApps")}</CardLabel>
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              {stats.top_apps.map((app) => (
                <span
                  key={app.name}
                  className="inline-flex items-center gap-1 rounded-md bg-mid-gray/10 px-2 py-0.5 text-xs"
                >
                  {app.name}
                  <span className="text-mid-gray tabular-nums">
                    {numberFormat.format(app.dictations)}
                  </span>
                </span>
              ))}
            </div>
          </div>
        )}
        {categoryTotal > 0 && (
          <p className="mt-3 text-[11px] text-mid-gray">
            {t("settings.insights.categoryHint")}
          </p>
        )}
      </Card>

      <Card>
        <div className="flex items-baseline justify-between gap-4 mb-4">
          <h2 className="text-lg font-semibold">
            {t("settings.insights.streakTitle", {
              total: numberFormat.format(stats.current_streak),
            })}
          </h2>
          <CardLabel>
            {t("settings.insights.longestStreak")}
            {" | "}
            {t("settings.insights.longestStreakDays", {
              total: numberFormat.format(stats.longest_streak),
            })}
          </CardLabel>
        </div>

        <div className="flex gap-2">
          <div className="flex flex-col shrink-0 text-[10px] text-mid-gray">
            {/* Spacer matching the month header, so the labels line up with
                the day rows next to them. */}
            <div className="mb-1 invisible leading-none">{"—"}</div>
            <div className="flex-1 grid grid-rows-7 gap-[3px]">
              {[0, 1, 2, 3, 4, 5, 6].map((row) => {
                const label = weekdayLabels.find((l) => l.row === row);
                return (
                  <div key={row} className="flex items-center leading-none">
                    {label ? label.label : ""}
                  </div>
                );
              })}
            </div>
          </div>
          <div className="flex-1 min-w-0">
            <div
              className="grid gap-[3px] mb-1 text-[10px] text-mid-gray"
              style={{
                gridTemplateColumns: `repeat(${CALENDAR_WEEKS}, minmax(0, 1fr))`,
              }}
            >
              {calendar.weeks.map((_, w) => {
                const start = calendar.monthStarts.get(w);
                return (
                  <div key={w} className="whitespace-nowrap overflow-visible">
                    {start ? monthFormat.format(start) : ""}
                  </div>
                );
              })}
            </div>
            <div
              className="grid gap-[3px]"
              style={{
                gridTemplateColumns: `repeat(${CALENDAR_WEEKS}, minmax(0, 1fr))`,
              }}
            >
              {calendar.weeks.map((week, w) => (
                <div key={w} className="grid grid-rows-7 gap-[3px]">
                  {week.map((cell) => {
                    if (cell.date === null) {
                      return <div key={cell.key} className="aspect-square" />;
                    }
                    const words = cell.activity?.words ?? 0;
                    const title = t("settings.insights.dayTooltip", {
                      date: dateFormat.format(cell.date),
                      words: numberFormat.format(words),
                      total: numberFormat.format(
                        cell.activity?.dictations ?? 0,
                      ),
                    });
                    return (
                      <div
                        key={cell.key}
                        title={title}
                        className={`aspect-square rounded-[3px] ${LEVEL_CLASSES[cell.level]} ${
                          cell.inCurrentStreak
                            ? "ring-2 ring-inset ring-text/60"
                            : ""
                        }`}
                      />
                    );
                  })}
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="mt-3 flex items-center justify-between text-[11px] text-mid-gray">
          <div className="flex items-center gap-1.5">
            <span>{t("settings.insights.more")}</span>
            {([4, 3, 2, 1, 0] as CalendarCell["level"][]).map((level) => (
              <span
                key={level}
                className={`inline-block w-3 h-3 rounded-[3px] ${LEVEL_CLASSES[level]}`}
              />
            ))}
            <span>{t("settings.insights.less")}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="inline-block w-3 h-3 rounded-[3px] ring-2 ring-inset ring-text/60" />
            <span>{t("settings.insights.currentStreak")}</span>
          </div>
        </div>
      </Card>
    </div>
  );
};
