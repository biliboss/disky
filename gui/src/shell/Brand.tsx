/** Sidebar brand: the snapshot's logo mark plus the product name "disky" (REQ-DCLEAN-032.A2). */
export function Brand() {
  return (
    <div class="brand">
      <svg class="brand-logo" viewBox="0 0 34 34" role="img" aria-label="disky logo">
        <rect x="0" y="0" width="34" height="34" rx="11" fill="var(--color-text)" />
        <circle
          cx="17"
          cy="17"
          r="9"
          fill="none"
          stroke="var(--color-accent)"
          stroke-dasharray="42.4 56.5"
          transform="rotate(-90 17 17)"
          stroke-width="5"
          stroke-linecap="round"
        />
        <circle
          cx="17"
          cy="17"
          r="9"
          fill="none"
          stroke="var(--color-accent-2-300)"
          stroke-dasharray="4 56.5"
          stroke-dashoffset="-48.5"
          transform="rotate(-90 17 17)"
          stroke-width="5"
          stroke-linecap="round"
        />
        <circle cx="17" cy="17" r="2.6" fill="var(--color-bg)" />
      </svg>
      <span class="brand-word">disky</span>
    </div>
  );
}
