import { useState, useEffect, useCallback, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useLocale } from "@/lib/i18n";

interface FeatureEntry {
  id: number;
  title: string;
  description: string;
  createdAt: string;
  votes: number;
}

interface FeatureListData {
  features: FeatureEntry[];
  totalVotes: number;
}

const STORAGE_KEY = "fluxdown-feature-votes";

const RANK_COLORS = ["#F5C518", "#C0C4CC", "#CD8C5C"]; // gold / silver / bronze

function loadVotedIds(): Set<number> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return new Set();
    const arr = JSON.parse(raw);
    if (Array.isArray(arr)) return new Set(arr.filter((n) => typeof n === "number"));
  } catch {
    // localStorage unavailable / malformed
  }
  return new Set();
}

function saveVotedIds(ids: Set<number>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...ids]));
  } catch {
    // localStorage unavailable
  }
}

export default function FeatureVotePage() {
  const { t } = useLocale();
  const [data, setData] = useState<FeatureListData | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [votedIds, setVotedIds] = useState<Set<number>>(new Set());
  const [pendingId, setPendingId] = useState<number | null>(null);
  const [statusMsg, setStatusMsg] = useState<{ text: string; type: "success" | "error" } | null>(null);
  const [showPropose, setShowPropose] = useState(false);
  const [proposeTitle, setProposeTitle] = useState("");
  const [proposeDesc, setProposeDesc] = useState("");
  const [proposing, setProposing] = useState(false);
  const [justCreatedId, setJustCreatedId] = useState<number | null>(null);
  const statusTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flashStatus = useCallback((text: string, type: "success" | "error") => {
    setStatusMsg({ text, type });
    if (statusTimer.current) clearTimeout(statusTimer.current);
    statusTimer.current = setTimeout(() => setStatusMsg(null), 4000);
  }, []);

  const refetch = useCallback(async (bustCache = false) => {
    const url = bustCache ? `/api/feature-vote?t=${Date.now()}` : "/api/feature-vote";
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const fresh: FeatureListData = await res.json();
    setData(fresh);
    return fresh;
  }, []);

  useEffect(() => {
    setVotedIds(loadVotedIds());
    refetch()
      .catch(() => setLoadError(true))
      .finally(() => setLoading(false));
    return () => {
      if (statusTimer.current) clearTimeout(statusTimer.current);
    };
  }, [refetch]);

  const handleToggleVote = useCallback(
    async (feature: FeatureEntry) => {
      if (pendingId !== null) return;
      const hasVoted = votedIds.has(feature.id);
      const action = hasVoted ? "unvote" : "vote";

      setPendingId(feature.id);

      // Optimistic update
      const delta = hasVoted ? -1 : 1;
      setData((prev) =>
        prev
          ? {
              totalVotes: prev.totalVotes + delta,
              features: prev.features.map((f) =>
                f.id === feature.id ? { ...f, votes: Math.max(0, f.votes + delta) } : f,
              ),
            }
          : prev,
      );
      const nextIds = new Set(votedIds);
      if (hasVoted) nextIds.delete(feature.id);
      else nextIds.add(feature.id);
      setVotedIds(nextIds);
      saveVotedIds(nextIds);

      try {
        const res = await fetch("/api/feature-vote", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ action, featureId: feature.id }),
        });

        if (res.status === 429) {
          flashStatus(t("featureVote.rateLimited"), "error");
          throw new Error("rate limited");
        }
        if (!res.ok) {
          flashStatus(t("featureVote.voteError"), "error");
          throw new Error(`HTTP ${res.status}`);
        }

        flashStatus(
          action === "vote" ? t("featureVote.voteSuccess") : t("featureVote.unvoteSuccess"),
          "success",
        );
        // Server cache was busted by the POST — pull authoritative counts
        refetch(true).catch(() => {});
      } catch {
        // Roll back optimistic update on failure
        setData((prev) =>
          prev
            ? {
                totalVotes: prev.totalVotes - delta,
                features: prev.features.map((f) =>
                  f.id === feature.id ? { ...f, votes: Math.max(0, f.votes - delta) } : f,
                ),
              }
            : prev,
        );
        const rollback = new Set(nextIds);
        if (hasVoted) rollback.add(feature.id);
        else rollback.delete(feature.id);
        setVotedIds(rollback);
        saveVotedIds(rollback);
      } finally {
        setPendingId(null);
      }
    },
    [pendingId, votedIds, t, flashStatus, refetch],
  );

  const handlePropose = useCallback(async () => {
    const title = proposeTitle.trim();
    if (!title || proposing) return;
    setProposing(true);
    try {
      const res = await fetch("/api/feature-vote", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          action: "propose",
          title,
          description: proposeDesc.trim(),
        }),
      });

      if (res.status === 429) {
        flashStatus(t("featureVote.rateLimited"), "error");
        return;
      }
      if (!res.ok) {
        flashStatus(t("featureVote.proposeError"), "error");
        return;
      }

      const result: { featureId: number } = await res.json();
      flashStatus(t("featureVote.proposeSuccess"), "success");
      setProposeTitle("");
      setProposeDesc("");
      setShowPropose(false);
      setJustCreatedId(result.featureId);
      // Cache already busted server-side — fetch the fresh list right away
      await refetch(true).catch(() => {});
    } catch {
      flashStatus(t("featureVote.proposeError"), "error");
    } finally {
      setProposing(false);
    }
  }, [proposeTitle, proposeDesc, proposing, t, flashStatus, refetch]);

  const maxVotes = data ? Math.max(1, ...data.features.map((f) => f.votes)) : 1;

  return (
    <section className="pt-24 sm:pt-32 pb-16 sm:pb-20">
      <div className="mx-auto max-w-3xl px-4 sm:px-6 lg:px-8">
        {/* ── Header ── */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5 }}
          className="text-center mb-10 sm:mb-14"
        >
          <span className="inline-flex items-center gap-2 rounded-full border border-dark-border bg-dark-surface1/50 px-4 py-1.5 text-xs font-medium text-dark-text-secondary backdrop-blur-sm mb-6">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-sky">
              <path d="M7 10v12" />
              <path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2a3.13 3.13 0 0 1 3 3.88Z" />
            </svg>
            {t("featureVote.badge")}
          </span>

          <h1 className="text-4xl sm:text-5xl font-bold tracking-tight leading-tight">
            <span className="text-dark-text">{t("featureVote.title")}</span>
            <span className="bg-gradient-to-r from-brand-sky to-brand-cyan bg-clip-text text-transparent">{t("featureVote.titleHighlight")}</span>
          </h1>

          <p className="mt-4 text-base sm:text-lg text-dark-text-secondary max-w-2xl mx-auto leading-relaxed">
            {t("featureVote.subtitle")}
          </p>
        </motion.div>

        {/* ── Propose bar ── */}
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, delay: 0.15 }}
          className="mb-8"
        >
          {!showPropose ? (
            <button
              onClick={() => setShowPropose(true)}
              className="w-full flex items-center justify-center gap-2 rounded-xl border border-dashed border-dark-border bg-dark-surface1/30 py-3.5 text-sm font-medium text-dark-text-secondary hover:border-brand-sky/50 hover:text-dark-text hover:bg-dark-surface1/60 transition-all"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M5 12h14" />
                <path d="M12 5v14" />
              </svg>
              {t("featureVote.proposeButton")}
            </button>
          ) : (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              className="rounded-xl border border-dark-border bg-dark-surface1/50 p-5 backdrop-blur-sm overflow-hidden"
            >
              <div className="space-y-3">
                <input
                  type="text"
                  value={proposeTitle}
                  onChange={(e) => setProposeTitle(e.target.value)}
                  maxLength={80}
                  placeholder={t("featureVote.proposeTitlePlaceholder")}
                  className="w-full rounded-lg border border-dark-border bg-dark-surface2/60 px-3.5 py-2.5 text-sm text-dark-text placeholder:text-dark-text-muted focus:outline-none focus:border-brand-sky/60 transition-colors"
                />
                <textarea
                  value={proposeDesc}
                  onChange={(e) => setProposeDesc(e.target.value)}
                  maxLength={1000}
                  rows={3}
                  placeholder={t("featureVote.proposeDescPlaceholder")}
                  className="w-full rounded-lg border border-dark-border bg-dark-surface2/60 px-3.5 py-2.5 text-sm text-dark-text placeholder:text-dark-text-muted focus:outline-none focus:border-brand-sky/60 transition-colors resize-none"
                />
                <div className="flex items-center justify-end gap-2">
                  <button
                    onClick={() => setShowPropose(false)}
                    className="rounded-lg px-4 py-2 text-sm font-medium text-dark-text-muted hover:text-dark-text transition-colors"
                  >
                    {t("featureVote.proposeCancel")}
                  </button>
                  <button
                    onClick={handlePropose}
                    disabled={!proposeTitle.trim() || proposing}
                    className="rounded-lg bg-gradient-to-r from-brand-sky to-brand-cyan px-5 py-2 text-sm font-semibold text-black disabled:opacity-40 disabled:cursor-not-allowed hover:opacity-90 transition-opacity"
                  >
                    {proposing ? t("featureVote.proposing") : t("featureVote.proposeSubmit")}
                  </button>
                </div>
              </div>
            </motion.div>
          )}
        </motion.div>

        {/* ── Status toast ── */}
        <AnimatePresence>
          {statusMsg && (
            <motion.p
              initial={{ opacity: 0, y: -6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              className={`mb-6 text-center text-sm font-medium ${statusMsg.type === "success" ? "text-success" : "text-danger"}`}
            >
              {statusMsg.text}
            </motion.p>
          )}
        </AnimatePresence>

        {/* ── Loading / error ── */}
        {loading && (
          <div className="flex items-center justify-center py-20">
            <div className="flex items-center gap-3 text-dark-text-muted">
              <svg className="w-5 h-5 animate-spin" viewBox="0 0 24 24" fill="none">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              <span className="text-sm">{t("featureVote.loading")}</span>
            </div>
          </div>
        )}

        {loadError && (
          <div className="flex items-center justify-center py-20">
            <span className="text-sm text-danger">{t("featureVote.loadError")}</span>
          </div>
        )}

        {/* ── Feature list ── */}
        {!loading && !loadError && data && (
          <>
            {data.features.length === 0 ? (
              <div className="text-center py-16 text-sm text-dark-text-muted">
                {t("featureVote.empty")}
              </div>
            ) : (
              <div className="space-y-3">
                <AnimatePresence initial={false}>
                  {data.features.map((feature, i) => {
                    const isVoted = votedIds.has(feature.id);
                    const isPending = pendingId === feature.id;
                    const isNew = justCreatedId === feature.id;
                    const pct = Math.round((feature.votes / maxVotes) * 100);
                    const rankColor = i < 3 && feature.votes > 0 ? RANK_COLORS[i] : undefined;

                    return (
                      <motion.div
                        key={feature.id}
                        layout
                        initial={{ opacity: 0, y: 12 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, scale: 0.97 }}
                        transition={{ duration: 0.35, delay: loading ? 0 : Math.min(0.05 * i, 0.3) }}
                        className={`group relative rounded-xl border overflow-hidden backdrop-blur-sm transition-colors ${
                          isNew
                            ? "border-brand-sky/60 ring-1 ring-brand-sky/30"
                            : isVoted
                              ? "border-brand-sky/40"
                              : "border-dark-border hover:border-dark-text-muted"
                        } bg-dark-surface1/40`}
                      >
                        {/* Relative popularity bar (background fill) */}
                        <motion.div
                          className="absolute inset-y-0 left-0 bg-gradient-to-r from-brand-sky/[0.07] to-brand-cyan/[0.04] pointer-events-none"
                          initial={false}
                          animate={{ width: `${pct}%` }}
                          transition={{ duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
                        />

                        <div className="relative flex items-center gap-4 p-4 sm:p-5">
                          {/* Rank */}
                          <div
                            className={`flex-shrink-0 flex items-center justify-center w-8 h-8 rounded-lg bg-dark-surface2 text-sm font-bold tabular-nums ${rankColor ? "" : "text-dark-text-muted"}`}
                            style={rankColor ? { color: rankColor } : undefined}
                          >
                            {i + 1}
                          </div>

                          {/* Title + description */}
                          <div className="flex-1 min-w-0">
                            <h3 className="text-sm sm:text-base font-semibold text-dark-text truncate">
                              {feature.title}
                            </h3>
                            {feature.description && (
                              <p className="mt-0.5 text-xs sm:text-sm text-dark-text-muted line-clamp-2 leading-relaxed">
                                {feature.description}
                              </p>
                            )}
                          </div>

                          {/* Vote button */}
                          <button
                            onClick={() => handleToggleVote(feature)}
                            disabled={isPending}
                            aria-pressed={isVoted}
                            className={`flex-shrink-0 flex flex-col items-center justify-center gap-0.5 w-16 h-16 rounded-xl border transition-all ${
                              isVoted
                                ? "border-brand-sky/60 bg-brand-sky/10 text-brand-sky"
                                : "border-dark-border bg-dark-surface2/60 text-dark-text-secondary hover:border-brand-sky/40 hover:text-dark-text hover:-translate-y-0.5"
                            } ${isPending ? "opacity-50 cursor-wait" : "cursor-pointer"}`}
                          >
                            <motion.svg
                              width="18"
                              height="18"
                              viewBox="0 0 24 24"
                              fill={isVoted ? "currentColor" : "none"}
                              stroke="currentColor"
                              strokeWidth="2"
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              animate={isVoted ? { scale: [1, 1.35, 1] } : {}}
                              transition={{ duration: 0.35 }}
                            >
                              <path d="m18 15-6-6-6 6" />
                            </motion.svg>
                            <span className="text-sm font-bold tabular-nums leading-none">
                              {feature.votes}
                            </span>
                          </button>
                        </div>
                      </motion.div>
                    );
                  })}
                </AnimatePresence>
              </div>
            )}

            <motion.p
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ delay: 0.4 }}
              className="mt-8 text-center text-sm text-dark-text-muted tabular-nums"
            >
              {t("featureVote.totalVotes", { n: String(data.totalVotes) })}
            </motion.p>
          </>
        )}
      </div>
    </section>
  );
}
