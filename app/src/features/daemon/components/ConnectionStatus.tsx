import type { DaemonConnectionStatus } from "../../../types";

type ConnectionStatusProps = {
  status: DaemonConnectionStatus;
  host?: string;
  port?: number;
  onClick?: () => void;
};

export function ConnectionStatus({
  status,
  host,
  port,
  onClick,
}: ConnectionStatusProps) {
  const indicator = getIndicator(status);
  const label = getLabel(status, host, port);

  return (
    <button
      type="button"
      className={`connection-status connection-status--${status}`}
      onClick={onClick}
      title={getTooltip(status, host, port)}
    >
      <span className="connection-status__indicator">{indicator}</span>
      <span className="connection-status__label">{label}</span>
    </button>
  );
}

function getIndicator(status: DaemonConnectionStatus): string {
  switch (status) {
    case "connected":
      return "●";
    case "connecting":
      return "○";
    case "disconnected":
    case "error":
      return "●";
  }
}

function getLabel(
  status: DaemonConnectionStatus,
  host?: string,
  port?: number,
): string {
  switch (status) {
    case "connected":
      return host && port ? `${host}:${port}` : "Connected";
    case "connecting":
      return "Connecting...";
    case "disconnected":
      return "Disconnected";
    case "error":
      return "Connection Error";
  }
}

function getTooltip(
  status: DaemonConnectionStatus,
  host?: string,
  port?: number,
): string {
  switch (status) {
    case "connected":
      return `Connected to ${host ?? "daemon"}:${port ?? "?"}`;
    case "connecting":
      return "Connecting to daemon...";
    case "disconnected":
      return "Click to configure daemon connection";
    case "error":
      return "Connection error - click to reconfigure";
  }
}
