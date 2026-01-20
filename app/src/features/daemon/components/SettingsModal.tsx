import { useCallback, useState } from "react";
import type { DaemonConnectionStatus } from "../../../types";

type SettingsModalProps = {
  isOpen: boolean;
  onClose: () => void;
  status: DaemonConnectionStatus;
  currentHost?: string;
  currentPort?: number;
  error?: string;
  onConfigure: (host: string, port: number, token: string) => Promise<void>;
  onConnect: () => Promise<void>;
  onDisconnect: () => Promise<void>;
};

const DEFAULT_HOST = "localhost";
const DEFAULT_PORT = 4733;

export function SettingsModal({
  isOpen,
  onClose,
  status,
  currentHost,
  currentPort,
  error,
  onConfigure,
  onConnect,
  onDisconnect,
}: SettingsModalProps) {
  const [host, setHost] = useState(currentHost ?? DEFAULT_HOST);
  const [port, setPort] = useState(currentPort ?? DEFAULT_PORT);
  const [token, setToken] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      setIsSubmitting(true);
      setLocalError(null);

      try {
        await onConfigure(host, port, token);
        await onConnect();
        onClose();
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setLocalError(message);
      } finally {
        setIsSubmitting(false);
      }
    },
    [host, port, token, onConfigure, onConnect, onClose],
  );

  const handleDisconnect = useCallback(async () => {
    setIsSubmitting(true);
    try {
      await onDisconnect();
    } finally {
      setIsSubmitting(false);
    }
  }, [onDisconnect]);

  if (!isOpen) {
    return null;
  }

  const displayError = localError ?? error;
  const isConnected = status === "connected";
  const isConnecting = status === "connecting" || isSubmitting;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__header">
          <h2>Daemon Connection</h2>
          <button
            type="button"
            className="modal__close"
            onClick={onClose}
            aria-label="Close"
          >
            ×
          </button>
        </div>

        <form onSubmit={handleSubmit} className="modal__body">
          <div className="form-group">
            <label htmlFor="daemon-host">Host</label>
            <input
              id="daemon-host"
              type="text"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              placeholder="localhost"
              disabled={isConnecting || isConnected}
            />
          </div>

          <div className="form-group">
            <label htmlFor="daemon-port">Port</label>
            <input
              id="daemon-port"
              type="number"
              value={port}
              onChange={(e) => setPort(Number(e.target.value))}
              placeholder="4733"
              min={1}
              max={65535}
              disabled={isConnecting || isConnected}
            />
          </div>

          <div className="form-group">
            <label htmlFor="daemon-token">Token</label>
            <input
              id="daemon-token"
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="Enter daemon token"
              disabled={isConnecting || isConnected}
            />
          </div>

          {displayError && (
            <div className="form-error">{displayError}</div>
          )}

          <div className="modal__footer">
            {isConnected ? (
              <button
                type="button"
                className="btn btn--secondary"
                onClick={handleDisconnect}
                disabled={isConnecting}
              >
                Disconnect
              </button>
            ) : (
              <button
                type="submit"
                className="btn btn--primary"
                disabled={isConnecting || !host || !port || !token}
              >
                {isConnecting ? "Connecting..." : "Connect"}
              </button>
            )}
            <button
              type="button"
              className="btn btn--ghost"
              onClick={onClose}
              disabled={isConnecting}
            >
              {isConnected ? "Close" : "Cancel"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
