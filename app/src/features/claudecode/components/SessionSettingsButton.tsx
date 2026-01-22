type SessionSettingsButtonProps = {
  onClick: () => void;
  disabled?: boolean;
};

/**
 * Gear icon button to open session settings modal.
 *
 * Reference: session-settings spec §Appendix
 */
export function SessionSettingsButton({
  onClick,
  disabled = false,
}: SessionSettingsButtonProps) {
  return (
    <button
      type="button"
      className="session-settings-btn"
      onClick={onClick}
      disabled={disabled}
      aria-label="Session settings"
      title="Session settings"
    >
      <svg
        width="16"
        height="16"
        viewBox="0 0 16 16"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden="true"
      >
        <path
          d="M6.5 1.5a.5.5 0 0 1 .5-.5h2a.5.5 0 0 1 .5.5v1.293a5.485 5.485 0 0 1 1.548.64l.916-.916a.5.5 0 0 1 .707 0l1.414 1.414a.5.5 0 0 1 0 .707l-.916.916c.28.478.496 1 .64 1.548H14.5a.5.5 0 0 1 .5.5v2a.5.5 0 0 1-.5.5h-1.293a5.485 5.485 0 0 1-.64 1.548l.916.916a.5.5 0 0 1 0 .707l-1.414 1.414a.5.5 0 0 1-.707 0l-.916-.916a5.485 5.485 0 0 1-1.548.64V14.5a.5.5 0 0 1-.5.5H7a.5.5 0 0 1-.5-.5v-1.293a5.485 5.485 0 0 1-1.548-.64l-.916.916a.5.5 0 0 1-.707 0l-1.414-1.414a.5.5 0 0 1 0-.707l.916-.916a5.485 5.485 0 0 1-.64-1.548H1.5a.5.5 0 0 1-.5-.5V7a.5.5 0 0 1 .5-.5h1.293c.144-.548.36-1.07.64-1.548l-.916-.916a.5.5 0 0 1 0-.707l1.414-1.414a.5.5 0 0 1 .707 0l.916.916A5.485 5.485 0 0 1 6.5 2.793V1.5zM8 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"
          fill="currentColor"
        />
      </svg>
    </button>
  );
}
