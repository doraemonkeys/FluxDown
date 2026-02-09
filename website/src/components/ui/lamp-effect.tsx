import { motion } from "framer-motion";
import { cn } from "@/lib/utils";

export function LampEffect({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "relative flex flex-col items-center justify-center overflow-hidden w-full",
        className,
      )}
    >
      {/* Lamp glow */}
      <div className="relative flex w-full flex-1 items-center justify-center isolate z-0">
        <motion.div
          initial={{ opacity: 0.5, width: "15rem" }}
          whileInView={{ opacity: 1, width: "30rem" }}
          transition={{ delay: 0.3, duration: 0.8, ease: "easeInOut" }}
          className="absolute inset-auto right-1/2 h-56 w-[30rem] bg-gradient-to-r from-transparent via-brand-sky/20 to-transparent blur-[100px]"
          style={{ transform: "translateX(50%)" }}
        />
        <motion.div
          initial={{ opacity: 0.5, width: "15rem" }}
          whileInView={{ opacity: 1, width: "30rem" }}
          transition={{ delay: 0.3, duration: 0.8, ease: "easeInOut" }}
          className="absolute inset-auto left-1/2 h-56 w-[30rem] bg-gradient-to-l from-transparent via-brand-cyan/20 to-transparent blur-[100px]"
          style={{ transform: "translateX(-50%)" }}
        />
        <motion.div
          initial={{ width: "8rem" }}
          whileInView={{ width: "16rem" }}
          transition={{ delay: 0.3, duration: 0.8, ease: "easeInOut" }}
          className="absolute inset-auto z-30 h-36 w-64 -translate-y-[6rem] rounded-full bg-brand-sky/10 blur-2xl"
        />
        <motion.div
          initial={{ width: "15rem" }}
          whileInView={{ width: "30rem" }}
          transition={{ delay: 0.3, duration: 0.8, ease: "easeInOut" }}
          className="absolute inset-auto z-50 h-0.5 w-[30rem] -translate-y-[7rem] bg-gradient-to-r from-transparent via-brand-sky to-transparent"
        />
      </div>

      {/* Content */}
      <div className="relative z-50">{children}</div>
    </div>
  );
}
