import { cn } from "@/lib/utils";

export function BentoGrid({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 mx-auto",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function BentoGridItem({
  className,
  title,
  description,
  header,
  icon,
}: {
  className?: string;
  title: string;
  description: string;
  header?: React.ReactNode;
  icon?: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "group relative rounded-xl border border-dark-border bg-dark-surface1/50 p-6 transition-all duration-300 hover:border-dark-surface3 hover:shadow-lg hover:shadow-brand-sky/[0.03] overflow-hidden",
        className,
      )}
    >
      {/* Hover gradient overlay */}
      <div className="absolute inset-0 rounded-xl opacity-0 group-hover:opacity-100 transition-opacity duration-500 bg-gradient-to-b from-brand-sky/[0.04] via-transparent to-transparent pointer-events-none" />

      {header && <div className="mb-4 relative z-10">{header}</div>}

      <div className="relative z-10">
        {icon && (
          <div className="mb-3 transition-transform duration-300 group-hover:scale-110 inline-block">
            {icon}
          </div>
        )}
        <h3 className="text-base font-semibold mb-2 text-dark-text group-hover:text-brand-sky transition-colors duration-300">
          {title}
        </h3>
        <p className="text-sm text-dark-text-secondary leading-relaxed">
          {description}
        </p>
      </div>
    </div>
  );
}
