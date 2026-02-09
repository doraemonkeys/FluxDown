import { motion } from "framer-motion";
import { BentoGrid, BentoGridItem } from "@/components/ui/bento-grid";
import { Cpu, Layers, Globe, Gauge, RefreshCw, Chrome } from "lucide-react";
import { useLocale } from "@/lib/i18n";

const IDMGridVisualization = () => {
  const colors = [
    "#3B82F6", "#22C55E", "#F59E0B", "#A855F7",
    "#06B6D4", "#EC4899", "#14B8A6", "#EF4444",
    "#8B5CF6", "#F97316", "#10B981", "#E11D48",
    "#0EA5E9", "#D946EF", "#84CC16", "#64748B",
  ];
  const cells = Array.from({ length: 64 }, (_, i) => {
    const segIdx = Math.floor(i / 4) % colors.length;
    // Deterministic "random" pattern to avoid SSR hydration mismatch
    const downloaded = !((i * 7 + 3) % 5 === 0);
    return { color: colors[segIdx], downloaded };
  });

  return (
    <div className="rounded-lg border border-dark-border bg-dark-surface2 p-2">
      <div className="grid grid-cols-16 gap-[1.5px]" style={{ gridTemplateColumns: "repeat(16, 1fr)" }}>
        {cells.map((cell, i) => (
          <motion.div
            key={i}
            className="aspect-square rounded-[1px]"
            style={{
              width: "5px",
              height: "5px",
              backgroundColor: cell.downloaded ? cell.color : `${cell.color}1F`,
            }}
            initial={{ opacity: 0, scale: 0 }}
            whileInView={{ opacity: 1, scale: 1 }}
            transition={{ delay: i * 0.008, duration: 0.2 }}
            viewport={{ once: true }}
          />
        ))}
      </div>
    </div>
  );
};

export default function FeaturesSection() {
  const { t } = useLocale();

  const features = [
    {
      title: t("features.rustTitle"),
      description: t("features.rustDesc"),
      icon: <div className="inline-flex items-center justify-center w-10 h-10 rounded-lg bg-[#38bdf8]/10"><Cpu className="w-5 h-5 text-[#38bdf8]" /></div>,
      className: "",
    },
    {
      title: t("features.segTitle"),
      description: t("features.segDesc"),
      icon: <div className="inline-flex items-center justify-center w-10 h-10 rounded-lg bg-[#06b6d4]/10"><Layers className="w-5 h-5 text-[#06b6d4]" /></div>,
      className: "lg:col-span-2",
      header: <IDMGridVisualization />,
    },
    {
      title: t("features.protoTitle"),
      description: t("features.protoDesc"),
      icon: <div className="inline-flex items-center justify-center w-10 h-10 rounded-lg bg-[#22C55E]/10"><Globe className="w-5 h-5 text-[#22C55E]" /></div>,
      className: "",
    },
    {
      title: t("features.speedTitle"),
      description: t("features.speedDesc"),
      icon: <div className="inline-flex items-center justify-center w-10 h-10 rounded-lg bg-[#F59E0B]/10"><Gauge className="w-5 h-5 text-[#F59E0B]" /></div>,
      className: "",
    },
    {
      title: t("features.resumeTitle"),
      description: t("features.resumeDesc"),
      icon: <div className="inline-flex items-center justify-center w-10 h-10 rounded-lg bg-[#A855F7]/10"><RefreshCw className="w-5 h-5 text-[#A855F7]" /></div>,
      className: "",
    },
    {
      title: t("features.browserTitle"),
      description: t("features.browserDesc"),
      icon: <div className="inline-flex items-center justify-center w-10 h-10 rounded-lg bg-[#EC4899]/10"><Chrome className="w-5 h-5 text-[#EC4899]" /></div>,
      className: "lg:col-span-2",
    },
  ];

  return (
    <section id="features" className="relative py-32 overflow-hidden">
      <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[800px] h-[400px] bg-brand-blue/[0.02] blur-[160px] rounded-full -z-10" />

      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <motion.div
          className="text-center max-w-2xl mx-auto mb-16"
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-100px" }}
          transition={{ duration: 0.5 }}
        >
          <span className="inline-flex items-center px-3 py-1 rounded-full text-xs font-semibold bg-[#38bdf8]/10 text-[#38bdf8] border border-[#38bdf8]/20 uppercase tracking-widest">
            {t("features.badge")}
          </span>
          <h2 className="mt-6 text-3xl sm:text-4xl lg:text-5xl font-bold tracking-tight text-dark-text">
            {t("features.title")}
            <span className="bg-gradient-to-r from-[#38bdf8] to-[#06b6d4] bg-clip-text text-transparent">{t("features.titleHighlight")}</span>
          </h2>
          <p className="mt-4 text-dark-text-secondary text-lg leading-relaxed">
            {t("features.subtitle")}
          </p>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 40 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-100px" }}
          transition={{ duration: 0.8, delay: 0.1 }}
        >
          <BentoGrid className="max-w-7xl">
            {features.map((f, i) => (
              <BentoGridItem key={i} title={f.title} description={f.description} icon={f.icon} header={f.header} className={f.className} />
            ))}
          </BentoGrid>
        </motion.div>
      </div>
    </section>
  );
}
