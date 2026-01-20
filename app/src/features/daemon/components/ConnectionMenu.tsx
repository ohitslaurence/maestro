import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { DaemonConnectionProfile, DaemonConnectionStatus } from "../../../types";
import { ConnectionStatus } from "./ConnectionStatus";

type ConnectionMenuProps = {
  status: DaemonConnectionStatus;
  host?: string;
  port?: number;
  profiles: DaemonConnectionProfile[];
  onConnectProfile: (profile: DaemonConnectionProfile) => Promise<void>;
  onDisconnect: () => Promise<void>;
  onManage: () => void;
};

const CONNECT_SHORTCUT = "Cmd/Ctrl + Shift + D";

export function ConnectionMenu({
  status,
  host,
  port,
  profiles,
  onConnectProfile,
  onDisconnect,
  onManage,
}: ConnectionMenuProps) {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const activeProfileId = useMemo(() => {
    if (!host || !port) {
      return null;
    }
    return `${host}:${port}`;
  }, [host, port]);

  const closeMenu = useCallback(() => {
    setIsOpen(false);
  }, []);

  const toggleMenu = useCallback(() => {
    setIsOpen((prev) => !prev);
  }, []);

  const handleConnectProfile = useCallback(
    async (profile: DaemonConnectionProfile) => {
      closeMenu();
      await onConnectProfile(profile);
    },
    [closeMenu, onConnectProfile],
  );

  const handleDisconnect = useCallback(async () => {
    closeMenu();
    await onDisconnect();
  }, [closeMenu, onDisconnect]);

  const handleManage = useCallback(() => {
    closeMenu();
    onManage();
  }, [closeMenu, onManage]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleClick = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!containerRef.current || !target) {
        return;
      }
      if (!containerRef.current.contains(target)) {
        setIsOpen(false);
      }
    };

    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setIsOpen(false);
      }
    };

    window.addEventListener("mousedown", handleClick);
    window.addEventListener("keydown", handleKey);

    return () => {
      window.removeEventListener("mousedown", handleClick);
      window.removeEventListener("keydown", handleKey);
    };
  }, [isOpen]);

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (!event.shiftKey) {
        return;
      }
      const isMeta = event.metaKey || event.ctrlKey;
      if (!isMeta || event.key.toLowerCase() !== "d") {
        return;
      }

      const target = event.target as HTMLElement | null;
      if (target) {
        const tag = target.tagName.toLowerCase();
        if (
          tag === "input" ||
          tag === "textarea" ||
          target.isContentEditable
        ) {
          return;
        }
      }

      event.preventDefault();
      setIsOpen(true);
    };

    window.addEventListener("keydown", handleShortcut);
    return () => {
      window.removeEventListener("keydown", handleShortcut);
    };
  }, []);

  return (
    <div className="connection-menu" ref={containerRef}>
      <ConnectionStatus
        status={status}
        host={host}
        port={port}
        onClick={toggleMenu}
      />
      {isOpen && (
        <div className="connection-menu__popover">
          <div className="connection-menu__header">
            <span>Connections</span>
            {status === "connected" && (
              <button
                type="button"
                className="btn btn--ghost btn--xs"
                onClick={handleDisconnect}
              >
                Disconnect
              </button>
            )}
          </div>
          {profiles.length === 0 ? (
            <div className="connection-menu__empty">No saved connections.</div>
          ) : (
            <div className="connection-menu__list">
              {profiles.map((profile) => {
                const isActive = activeProfileId === profile.id;
                const displayName = profile.name?.trim();
                return (
                  <button
                    key={profile.id}
                    type="button"
                    className={`connection-menu__item${isActive ? " connection-menu__item--active" : ""}`}
                    onClick={() => handleConnectProfile(profile)}
                    disabled={isActive}
                  >
                    <span className="connection-menu__item-meta">
                      <span className="connection-menu__item-label">
                        {displayName ?? `${profile.host}:${profile.port}`}
                      </span>
                      {displayName && (
                        <span className="connection-menu__item-subtitle">
                          {profile.host}:{profile.port}
                        </span>
                      )}
                    </span>
                    <span className="connection-menu__item-action">
                      {isActive ? "Active" : "Connect"}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
          <button
            type="button"
            className="btn btn--secondary btn--block"
            onClick={handleManage}
          >
            Manage connections
          </button>
          <div className="connection-menu__shortcut">{CONNECT_SHORTCUT}</div>
        </div>
      )}
    </div>
  );
}
