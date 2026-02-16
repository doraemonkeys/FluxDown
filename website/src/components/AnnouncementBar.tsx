import { useState, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useLocale } from "@/lib/i18n";
import { ANNOUNCEMENTS } from "@/lib/announcements";

const STORAGE_KEY = "fluxdown-dismissed-announcements";

function getDismissed(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function setDismissed(ids: string[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
  } catch {
    // localStorage unavailable
  }
}

export default function AnnouncementBar() {
  const { t } = useLocale();
  const [visible, setVisible] = useState(false);
  const [current, setCurrent] = useState<Announcement | null>(null);

  useEffect(() => {
    const dismissed = getDismissed();
    const active = ANNOUNCEMENTS.find((a) => a.active && !dismissed.includes(a.id));
    if (active) {
      setCurrent(active);
      setVisible(true);
    }
  }, []);

  const handleDismiss = useCallback(() => {
    if (!current) return;
    setVisible(false);
    const dismissed = getDismissed();
    setDismissed([...dismissed, current.id]);
  }, [current]);

  return (
    <AnimatePresence>
      {visible && current && (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
          className="sticky top-0 z-[60] overflow-hidden"
        >
          <div className="relative flex items-center justify-center gap-3 bg-gradient-to-r from-brand-blue/15 via-brand-sky/10 to-brand-cyan/15 border-b border-brand-blue/20 px-4 py-2.5">
            <div className="absolute inset-0 bg-dark-bg/60 backdrop-blur-sm -z-10" />

            <span className="relative flex h-2 w-2 shrink-0">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-brand-sky opacity-75" />
              <span className="relative inline-flex h-2 w-2 rounded-full bg-brand-sky" />
            </span>

            <p className="text-xs sm:text-sm text-dark-text-secondary text-center leading-relaxed">
              {current.link ? (
                <a
                  href={current.link}
                  className="hover:text-dark-text transition-colors underline underline-offset-2 decoration-brand-sky/40 hover:decoration-brand-sky"
                >
                  {t(current.messageKey)}
                </a>
              ) : (
                <span>{t(current.messageKey)}</span>
              )}
            </p>

            <button
              onClick={handleDismiss}
              className="shrink-0 flex items-center justify-center w-6 h-6 rounded-full hover:bg-dark-surface3/50 transition-colors cursor-pointer"
              aria-label={t("announcement.close")}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="text-dark-text-muted"
              >
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
