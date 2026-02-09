import { useLocale } from "@/lib/i18n";

export default function Footer() {
  const { t } = useLocale();
  const year = new Date().getFullYear();

  return (
    <footer className="border-t border-dark-border bg-dark-surface1">
      <div className="mx-auto max-w-7xl px-6 lg:px-8 py-12">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-8">
          {/* Brand */}
          <div className="md:col-span-2">
            <a href="/" className="flex items-center gap-2.5 mb-4">
              <img src="/logo.svg" alt="FluxDown" className="h-8 w-8" />
              <span className="text-lg font-semibold tracking-tight">
                <span className="text-brand-sky">Flux</span><span className="text-dark-text">Down</span>
              </span>
            </a>
            <p className="text-sm text-dark-text-secondary max-w-md leading-relaxed">
              {t("footer.desc")}
            </p>
          </div>

          {/* Links */}
          <div>
            <h3 className="text-sm font-semibold text-dark-text mb-4">{t("footer.product")}</h3>
            <ul className="space-y-2.5">
              <li><a href="#features" className="text-sm text-dark-text-secondary hover:text-dark-text transition-colors">{t("footer.features")}</a></li>
              <li><a href="#extension" className="text-sm text-dark-text-secondary hover:text-dark-text transition-colors">{t("footer.browserExtension")}</a></li>
              <li><a href="#download" className="text-sm text-dark-text-secondary hover:text-dark-text transition-colors">{t("footer.download")}</a></li>
            </ul>
          </div>

          <div>
            <h3 className="text-sm font-semibold text-dark-text mb-4">{t("footer.support")}</h3>
            <ul className="space-y-2.5">
              <li><a href="#" className="text-sm text-dark-text-secondary hover:text-dark-text transition-colors">{t("footer.documentation")}</a></li>
              <li><a href="#" className="text-sm text-dark-text-secondary hover:text-dark-text transition-colors">{t("footer.faq")}</a></li>
              <li><a href="#" className="text-sm text-dark-text-secondary hover:text-dark-text transition-colors">{t("footer.contact")}</a></li>
            </ul>
          </div>
        </div>

        {/* Bottom */}
        <div className="mt-12 pt-6 border-t border-dark-border flex flex-col sm:flex-row items-center justify-between gap-4">
          <p className="text-xs text-dark-text-muted">
            {t("footer.copyright", { year: String(year) })}
          </p>
          <div className="flex items-center gap-1 text-xs text-dark-text-muted">
            {t("footer.builtWith")}
            <svg className="h-3 w-3 text-danger mx-0.5" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z" />
            </svg>
            {t("footer.using")}
          </div>
        </div>
      </div>
    </footer>
  );
}
