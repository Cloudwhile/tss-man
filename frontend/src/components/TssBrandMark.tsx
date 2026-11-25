import { useId } from 'react';

/* Precision TeamSpeak acoustic emblem with inner specular bevel for light and dark surfaces. */
export function TssBrandMark({ className = 'w-7 h-7' }: { className?: string }) {
  const bgId = useId();
  const glowId = useId();

  return (
    <svg
      viewBox="0 0 36 36"
      fill="none"
      aria-hidden="true"
      className={`shrink-0 ${className}`}
    >
      <defs>
        <linearGradient id={bgId} x1="4" y1="2" x2="32" y2="35" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#3b82f6" />
          <stop offset="52%" stopColor="#2563eb" />
          <stop offset="100%" stopColor="#1e40af" />
        </linearGradient>
        <radialGradient id={glowId} cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(18 7) rotate(90) scale(18 22)">
          <stop offset="0%" stopColor="#93c5fd" stopOpacity="0.38" />
          <stop offset="100%" stopColor="#2563eb" stopOpacity="0" />
        </radialGradient>
      </defs>
      <rect x="1" y="1" width="34" height="34" rx="9" fill={`url(#${bgId})`} />
      <rect x="1" y="1" width="34" height="34" rx="9" fill={`url(#${glowId})`} />
      <rect
        x="1.5"
        y="1.5"
        width="33"
        height="33"
        rx="8.5"
        stroke="#ffffff"
        strokeOpacity="0.22"
      />
      {/* Acoustic bridge arc */}
      <path
        d="M9.75 18.75C9.75 13.78 13.44 10 18 10C22.56 10 26.25 13.78 26.25 18.75"
        stroke="#ffffff"
        strokeWidth="2.2"
        strokeLinecap="round"
      />
      {/* Precision equalizer / voice signal bars */}
      <rect x="13" y="19" width="2.2" height="6.5" rx="1.1" fill="#dbeafe" />
      <rect x="16.9" y="15.8" width="2.2" height="10.2" rx="1.1" fill="#ffffff" />
      <rect x="20.8" y="19" width="2.2" height="6.5" rx="1.1" fill="#dbeafe" />
      {/* Headset capsules */}
      <rect x="8.2" y="17.6" width="2.9" height="6.8" rx="1.45" fill="#ffffff" />
      <rect x="24.9" y="17.6" width="2.9" height="6.8" rx="1.45" fill="#ffffff" />
    </svg>
  );
}
