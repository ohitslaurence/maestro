import { useCallback, useState } from "react";
import type {
  DaemonConnectionProfile,
  DaemonConnectionStatus,
} from "../../../types";

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
  profiles: DaemonConnectionProfile[];
  rememberLastUsed: boolean;
  onRememberLastUsedChange: (value: boolean) => void;
  onConnectProfile: (profile: DaemonConnectionProfile) => Promise<void>;
  onRemoveProfile: (profileId: string) => void;
  onSaveProfile: (input: {
    id?: string | null;
    name?: string;
    host: string;
    port: number;
    token: string;
  }) => DaemonConnectionProfile;
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
  profiles,
  rememberLastUsed,
  onRememberLastUsedChange,
  onConnectProfile,
  onRemoveProfile,
  onSaveProfile,
}: SettingsModalProps) {
  const [name, setName] = useState("");
  const [host, setHost] = useState(currentHost ?? DEFAULT_HOST);
  const [port, setPort] = useState(currentPort ?? DEFAULT_PORT);
  const [token, setToken] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      setIsSubmitting(true);
      setLocalError(null);

      try {
        await onConfigure(host, port, token);
        await onConnect();
        const saved = onSaveProfile({
          id: editingProfileId,
          name,
          host,
          port,
          token,
        });
        setEditingProfileId(saved.id);
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

  const handleSaveProfile = useCallback(() => {
    const saved = onSaveProfile({
      id: editingProfileId,
      name,
      host,
      port,
      token,
    });
    setEditingProfileId(saved.id);
  }, [editingProfileId, host, port, token, name, onSaveProfile]);

  const handleEditProfile = useCallback((profile: DaemonConnectionProfile) => {
    setName(profile.name ?? "");
    setHost(profile.host);
    setPort(profile.port);
    setToken(profile.token);
    setEditingProfileId(profile.id);
    setLocalError(null);
  }, []);

  const handleNewProfile = useCallback(() => {
    setName("");
    setHost(currentHost ?? DEFAULT_HOST);
    setPort(currentPort ?? DEFAULT_PORT);
    setToken("");
    setEditingProfileId(null);
    setLocalError(null);
  }, [currentHost, currentPort]);

  const handleRemoveProfile = useCallback(
    (profileId: string) => {
      if (editingProfileId === profileId) {
        handleNewProfile();
      }
      onRemoveProfile(profileId);
    },
    [editingProfileId, handleNewProfile, onRemoveProfile],
  );

  const handleConnectProfile = useCallback(
    async (profile: DaemonConnectionProfile) => {
      setIsSubmitting(true);
      setLocalError(null);
      try {
        await onConnectProfile(profile);
        onClose();
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setLocalError(message);
      } finally {
        setIsSubmitting(false);
      }
    },
    [onConnectProfile, onClose],
  );

  if (!isOpen) {
    return null;
  }

  const displayError = localError ?? error;
  const isConnected = status === "connected";
  const isConnecting = status === "connecting" || isSubmitting;
  const activeProfileId =
    currentHost && currentPort ? `${currentHost}:${currentPort}` : null;
  const isEditing = !!editingProfileId;

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
            <label htmlFor="daemon-name">Name</label>
            <input
              id="daemon-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Production daemon"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="none"
              spellCheck={false}
              disabled={isConnecting}
            />
          </div>

          <div className="form-group">
            <label htmlFor="daemon-host">Host</label>
            <input
              id="daemon-host"
              type="text"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              placeholder="localhost"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="none"
              spellCheck={false}
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
              autoComplete="off"
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
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="none"
              spellCheck={false}
              disabled={isConnecting || isConnected}
            />
          </div>

          <div className="saved-connections">
            <div className="saved-connections__header">
              <h3>Saved Connections</h3>
              <button
                type="button"
                className="btn btn--ghost btn--xs"
                onClick={handleNewProfile}
                disabled={isConnecting}
              >
                New
              </button>
              <label className="saved-connections__toggle">
                <input
                  type="checkbox"
                  checked={rememberLastUsed}
                  onChange={(e) => onRememberLastUsedChange(e.target.checked)}
                  disabled={isConnecting}
                />
                Remember last used
              </label>
            </div>

            {profiles.length === 0 ? (
              <p className="saved-connections__empty">No saved connections yet.</p>
            ) : (
              <ul className="saved-connections__list">
                {profiles.map((profile) => {
                  const isActive =
                    isConnected && activeProfileId === profile.id;
                  return (
                    <li key={profile.id} className="saved-connections__item">
                      <div className="saved-connections__meta">
                        <span className="saved-connections__host">
                          {profile.name?.trim()
                            ? profile.name
                            : `${profile.host}:${profile.port}`}
                        </span>
                        {profile.name?.trim() && (
                          <span className="saved-connections__host-sub">
                            {profile.host}:{profile.port}
                          </span>
                        )}
                        <span className="saved-connections__hint">Token saved</span>
                      </div>
                      <div className="saved-connections__actions">
                        <button
                          type="button"
                          className="btn btn--ghost btn--xs"
                          onClick={() => handleConnectProfile(profile)}
                          disabled={isConnecting || isActive}
                        >
                          {isActive ? "Active" : "Connect"}
                        </button>
                        <button
                          type="button"
                          className="btn btn--ghost btn--xs"
                          onClick={() => handleEditProfile(profile)}
                          disabled={isConnecting}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          className="btn btn--ghost btn--xs"
                          onClick={() => handleRemoveProfile(profile.id)}
                          disabled={isConnecting}
                        >
                          Remove
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
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
              onClick={handleSaveProfile}
              disabled={isConnecting || !host || !port || !token}
            >
              {isEditing ? "Update Profile" : "Save Profile"}
            </button>
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
