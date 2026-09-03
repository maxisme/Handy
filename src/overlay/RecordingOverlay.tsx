import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";

type OverlayState =
  | "recording"
  | "streaming"
  | "transcribing"
  | "processing"
  | "copy-prompt"
  | "learned";

// Payload of `learned-words-event` (overlay.rs `LearnedWordsEvent`): a batch
// of words learned from a correction in another app, and how long the toast
// counts down before dismissing itself.
type LearnedWordsEvent = {
  batch_id: number;
  words: string[];
  timeout_ms: number;
};

type LearnedToast = {
  batchId: number;
  words: string[];
  timeoutMs: number;
};

// Number of reactive bars in the waveform (the simple, smoothed style shared by
// every overlay form). Mic levels arrive as 16 FFT buckets; we take the first N.
const WAVE_BARS = 9;

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // `Stream::play()` returning does not mean hardware callbacks are flowing.
  // Stay visually in an arming state until the backend processes the first
  // actual microphone sample chunk.
  const [captureReady, setCaptureReady] = useState(false);
  // Whether the recording outlives the shortcut key (a tap, toggle mode, or the
  // pin button). Unpinned: releasing the key ends it, so offer the pin. Pinned:
  // only the next press ends it, so offer the finish tick instead.
  const [pinned, setPinned] = useState(false);
  const [levels, setLevels] = useState<number[]>(Array(WAVE_BARS).fill(0));
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = useState(0);
  // Overlay placement (top vs bottom of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one.
  const [position, setPosition] = useState<"top" | "bottom">("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = useState(false);
  // Copy prompt: flips to true once the transcript is on the clipboard so the
  // button can confirm before the backend hides the overlay.
  const [copied, setCopied] = useState(false);
  // Learned-words toast: the batch on offer, whether Undo has taken it back,
  // whether an undo is in flight, and whether the pointer is over the pill
  // (which holds the countdown).
  const [learned, setLearned] = useState<LearnedToast | null>(null);
  const [undone, setUndone] = useState(false);
  const [undoing, setUndoing] = useState(false);
  const [toastPaused, setToastPaused] = useState(false);
  // Countdown bookkeeping: which batch the timer belongs to and how much of
  // its timeout is left, so a hover pause resumes where it stopped.
  const toastBatchRef = useRef<number | null>(null);
  const toastRemainingRef = useRef(0);

  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  // Live-text scroll-back: the text region "sticks" to the newest line while the
  // user is at the bottom; if they scroll up to read history, auto-follow pauses
  // until they scroll back down.
  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const direction = getLanguageDirection(i18n.language);

  useEffect(() => {
    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        const overlayState = event.payload as OverlayState;
        // Reset synchronously before settings I/O. A fast microphone can emit
        // recording-ready while the awaits below are in flight; resetting after
        // them would overwrite that event and leave the overlay stuck arming.
        if (overlayState === "recording" || overlayState === "streaming") {
          setCaptureReady(false);
          setPinned(false);
          smoothedLevelsRef.current = Array(16).fill(0);
          setLevels(Array(WAVE_BARS).fill(0));
          setStreamText({ committed: "", tentative: "" });
        }
        if (overlayState === "copy-prompt") {
          setCopied(false);
        }
        if (overlayState === "learned") {
          setUndone(false);
          setUndoing(false);
          setToastPaused(false);
        }

        await syncLanguageFromSettings();
        // The Live panel flows downward from a top overlay and upward from a
        // bottom one; read the placement so the layout can flip to match.
        try {
          const settings = await commands.getAppSettings();
          if (settings.status === "ok") {
            setPosition(
              settings.data.overlay_position === "top" ? "top" : "bottom",
            );
          }
        } catch {
          // Keep the previous/default placement if settings can't be read.
        }
        setState(overlayState);
        if (overlayState === "streaming") {
          setPhase("listening");
          setWorkKind("transcribing");
          setElapsed(0);
          setSession((s) => s + 1); // remount the card fresh for this session
        }
        setIsVisible(true);
      });

      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        setCaptureReady(false);
      });

      const unlistenReady = await listen("recording-ready", () => {
        setElapsed(0);
        setCaptureReady(true);
      });

      const unlistenPinned = await listen<boolean>(
        "recording-pinned",
        (event) => {
          setPinned(event.payload);
        },
      );

      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const newLevels = event.payload as number[];
        // Exponential smoothing across the 16 buckets, then take the first N
        // bars for the shared waveform.
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3;
        });
        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, WAVE_BARS));
      });

      const unlistenLearned = await listen<LearnedWordsEvent>(
        "learned-words-event",
        (event) => {
          setLearned({
            batchId: event.payload.batch_id,
            words: event.payload.words,
            timeoutMs: event.payload.timeout_ms,
          });
        },
      );

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        setStreamText(event.payload);
      });

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
      });

      return () => {
        unlistenShow();
        unlistenHide();
        unlistenReady();
        unlistenPinned();
        unlistenLevel();
        unlistenLearned();
        unlistenStream();
        unlistenPhase();
      };
    };

    setupEventListeners();
  }, []);

  // Elapsed capture timer starts only once microphone samples are flowing.
  useEffect(() => {
    if (state !== "streaming" || !isVisible || !captureReady) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, isVisible, captureReady]);

  // Learned-words countdown. Runs only while the toast is up, not undone and
  // not hovered; a hover pause stores the time left so the timer resumes from
  // there instead of restarting. When it runs out the backend hides the toast.
  useEffect(() => {
    if (
      state !== "learned" ||
      !isVisible ||
      learned === null ||
      undone ||
      toastPaused
    ) {
      return;
    }
    if (toastBatchRef.current !== learned.batchId) {
      toastBatchRef.current = learned.batchId;
      toastRemainingRef.current = learned.timeoutMs;
    }
    const remaining = toastRemainingRef.current;
    const startedAt = Date.now();
    const id = setTimeout(() => {
      commands.dismissLearnedToast();
    }, remaining);
    return () => {
      clearTimeout(id);
      toastRemainingRef.current = Math.max(
        0,
        remaining - (Date.now() - startedAt),
      );
    };
  }, [state, isVisible, learned, undone, toastPaused]);

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    // Fade the top edge only once text actually overflows the cap.
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  if (!isVisible) return null;

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  // ---- Shared building blocks (one visual language for every overlay form) ----
  const waveform = (
    <div className={`swave ${captureReady ? "ready" : "arming"}`}>
      {levels.map((v, i) => (
        <i
          key={i}
          style={{
            height: `${Math.max(3, Math.min(18, 3 + Math.pow(v, 0.7) * 15))}px`,
          }}
        />
      ))}
    </div>
  );

  const closeBtn = (label: string, onClick: () => void) => (
    <button className="sx" aria-label={label} onClick={onClick}>
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );

  const cancelBtn = closeBtn("cancel", () => commands.cancelOperation());

  // Pin: keep recording after the shortcut key is released. Shown while the
  // release would end the recording (a push-to-talk hold).
  const pinBtn = (
    <button
      className="sx spin"
      aria-label={t("overlay.pin")}
      title={t("overlay.pin")}
      onClick={() => commands.pinRecording()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M5.5 2.5 H10.5 M6.5 2.5 V6.5 L4 9.5 H12 L9.5 6.5 V2.5 M8 9.5 V14"
          stroke="currentColor"
          strokeWidth="1.6"
          fill="none"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </button>
  );

  // Finish: stop and transcribe. Shown once the recording outlives the key
  // (pinned, a tap, or toggle mode) — the same as pressing the shortcut again.
  const finishBtn = (
    <button
      className="sx sdone"
      aria-label={t("overlay.finish")}
      title={t("overlay.finish")}
      onClick={() => commands.finishRecording()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M3.5 8.5 L6.5 11.5 L12.5 5"
          stroke="currentColor"
          strokeWidth="1.8"
          fill="none"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </button>
  );

  // pin/finish (left) | waveform (center) | timer + cancel (right) — same
  // structure for pill & panel, so the Live morph is a pure width change.
  const listeningRow = (showTimer: boolean, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">{pinned ? finishBtn : pinBtn}</div>
      {waveform}
      <div className="sbase-r">
        {showTimer && <span className="stimer">{fmtTime(elapsed)}</span>}
        {showCancel && cancelBtn}
      </div>
    </div>
  );

  // spinner (left) | label (center) | cancel (right) — same 3-zone grid as the
  // listening row, so the label is centered.
  const workingRow = (label: string, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className="sspinner" />
      </div>
      <span className="swork-label">{label}</span>
      <div className="sbase-r">{showCancel && cancelBtn}</div>
    </div>
  );

  // ---- Live overlay: a pill that sculpts open into a panel ----
  if (state === "streaming") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const working = phase === "working";
    // Keep the panel open whenever there's text — even while finalizing — so the
    // transcript stays put under a working spinner instead of collapsing and
    // squishing the text mid-stream. Only fall back to the small working pill
    // when there was no text to preserve.
    const open = hasText;
    const collapsed = working && !hasText;

    return (
      <div dir={direction} className={`ov-stage ${position}`}>
        <div
          key={session}
          className={`scard ${open ? "open" : ""} ${collapsed ? "working" : ""} ${
            isVisible ? "" : "leaving"
          }`}
        >
          <div className="stext">
            <div className="stext-clip">
              <div
                className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                ref={capRef}
                onScroll={handleStreamScroll}
              >
                <p>
                  <span className="committed">
                    {streamText.committed ? streamText.committed + " " : ""}
                  </span>
                  <span className="tentative">{streamText.tentative}</span>
                  {/* Drop the blinking caret once finalizing — it's no longer
                      capturing, and a static spinner conveys the work. */}
                  {!working && <span className="scaret" />}
                </p>
              </div>
            </div>
          </div>
          {working
            ? workingRow(
                workKind === "polishing"
                  ? t("overlay.processing")
                  : t("overlay.transcribing"),
                true,
              )
            : listeningRow(open, true)}
        </div>
      </div>
    );
  }

  // ---- Copy prompt: the transcript finished but nothing editable was focused
  // (or the paste failed), so offer it for the clipboard instead. Same compact
  // pill as the working state: empty (left) | button (center) | dismiss (right);
  // the empty left cell keeps the button centered.
  if (state === "copy-prompt") {
    const handleCopy = async () => {
      if (copied) return;
      const result = await commands.copyLastTranscript();
      if (result.status === "ok") setCopied(true);
    };
    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
      >
        <div className="scard compact ccopy">
          <div className="sbase">
            <div className="sbase-l" />
            <button
              className={`scopy ${copied ? "done" : ""}`}
              onClick={handleCopy}
              disabled={copied}
            >
              {copied ? t("overlay.copied") : t("overlay.copyLastTranscript")}
            </button>
            <div className="sbase-r">
              {closeBtn(t("overlay.dismiss"), () =>
                commands.dismissCopyPrompt(),
              )}
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ---- Learned-words toast: a correction in another app taught Handy new
  // words. label (start) | Undo | dismiss (end), with a countdown bar along the
  // bottom edge. Hovering the pill holds the countdown; Undo takes the batch
  // back and the label turns into the confirmation until the backend hides.
  if (state === "learned") {
    const label = (() => {
      if (undone) return t("overlay.undone");
      if (learned === null) return "";
      if (learned.words.length === 1) {
        return t("overlay.learnedOne", { word: learned.words[0] });
      }
      return t("overlay.learnedMany", { count: learned.words.length });
    })();
    const handleUndo = async () => {
      if (undone || undoing) return;
      setUndoing(true);
      const result = await commands.undoLearnedToast();
      setUndoing(false);
      if (result.status === "ok") setUndone(true);
    };
    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
      >
        <div
          className={`scard compact clearned ${toastPaused ? "paused" : ""}`}
          onMouseEnter={() => setToastPaused(true)}
          onMouseLeave={() => setToastPaused(false)}
        >
          <div className="sbase">
            <span
              className={`slearned-label ${undone ? "done" : ""}`}
              title={label}
            >
              {label}
            </span>
            {!undone && (
              <button className="scopy" onClick={handleUndo} disabled={undoing}>
                {t("overlay.undo")}
              </button>
            )}
            <div className="sbase-r">
              {closeBtn(t("overlay.dismiss"), () =>
                commands.dismissLearnedToast(),
              )}
            </div>
          </div>
          {learned !== null && !undone && (
            <div
              key={learned.batchId}
              className="sbar"
              style={{ animationDuration: `${learned.timeoutMs}ms` }}
            />
          )}
        </div>
      </div>
    );
  }

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing). Never both. The pill animates its
  // width between them; the cancel button is in both rows so it stays put.
  const working = state === "transcribing" || state === "processing";
  const workLabel =
    state === "processing"
      ? t("overlay.processing")
      : t("overlay.transcribing");

  return (
    <div
      dir={direction}
      className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
    >
      <div
        className={`scard compact ${working && isVisible ? "cworking" : ""}`}
      >
        {working ? workingRow(workLabel, true) : listeningRow(false, true)}
      </div>
    </div>
  );
};

export default RecordingOverlay;
