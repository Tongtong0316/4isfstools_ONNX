function icon(paths: React.ReactNode, extra?: React.ReactNode) {
  return ({ className = "" }: { className?: string }) => (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      {paths}
      {extra}
    </svg>
  );
}

export const MicIcon = icon(
  <>
    <rect x="9" y="2" width="6" height="11" rx="3" stroke="currentColor" strokeWidth="1.8" />
    <path d="M5 11a7 7 0 0 0 14 0" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    <path d="M12 19v3" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  </>
);

export const MusicNoteIcon = icon(
  <path d="M9 18.5a2.5 2.5 0 1 1-2-2.45V6.5l10-2v9.55a2.5 2.5 0 1 1-2-2.45V8.4l-6 1.2v8.9Z" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
);

export const FolderIcon = icon(
  <>
    <path d="M4.5 7.5h5l1.8 2h8.2v8.8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2V9.5a2 2 0 0 1 2-2Z" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
  </>
);

export const RefreshIcon = icon(
  <path d="M4 12a8 8 0 0 1 14.3-5M20 12a8 8 0 0 1-14.1 5.2" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  , <path d="M20 4v4h-4M4 20v-4h4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
);

export const SearchIcon = icon(
  <path d="m17.2 17.2 3.3 3.3M10.8 18a7.2 7.2 0 1 1 0-14.4 7.2 7.2 0 0 1 0 14.4Z" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" />
);

export const RocketIcon = icon(
  <>
    <path d="M12 15.5V22M9 3.5l3-1.5 3 1.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M5 12c0-4 3-8 7-9 4 1 7 5 7 9v1H5v-1Z" stroke="currentColor" strokeWidth="1.8" />
    <path d="M5 13h14" stroke="currentColor" strokeWidth="1.8" />
    <path d="M10 13s-.5 2.5-2 4.5h8c-1.5-2-2-4.5-2-4.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  </>
);

export const LaptopIcon = icon(
  <>
    <rect x="3" y="4" width="18" height="12" rx="2" stroke="currentColor" strokeWidth="1.8" />
    <path d="M2 20h20" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  </>
);

export const TargetIcon = icon(
  <>
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" />
    <circle cx="12" cy="12" r="5" stroke="currentColor" strokeWidth="1.8" />
    <circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none" />
  </>
);

export const FileIcon = icon(
  <>
    <path d="M6.5 3.5h7l4 4v13h-11v-17Z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
    <path d="M13.5 3.5v4h4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
  </>
);

export const SettingsIcon = icon(
  <path d="M12 2.5 16 4l4-1 1.5 3.5L20 10l1.5 3.5L20 17 19 21l-5 .5-4-.5-2.5 2.5L6 18.5 4 16 2.5 13 4 10 2 6l3-2.5L8 2.5l2.5-2 2 2Z M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
);

export const CheckIcon = icon(
  <path d="M5 13l4 4L19 7" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
);

export const InfoIcon = icon(
  <>
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" />
    <path d="M12 8v1M12 11v5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  </>
);
