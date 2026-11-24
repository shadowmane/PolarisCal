// Session state for the whole app: decides whether to show setup, sign-in, a forced
// password change, the lock screen, or the app itself, and runs the idle auto-lock.

import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import type { AuthStatus } from "../bindings/AuthStatus";
import type { User } from "../bindings/User";
import {
  ChangePasswordScreen,
  ConnectionErrorScreen,
  LockScreen,
  SetupScreen,
  SignInScreen,
  SplashScreen,
} from "../components/AuthScreens";
import { UpdateNotice } from "../components/UpdateNotice";
import { ApiError, AUTH_CHANGED_EVENT, api } from "./api";
import { setPasswordPolicy } from "./password";
import { can as roleCan, type Permission } from "./permissions";

interface AuthContextValue {
  user: User;
  idleLockMinutes: number;
  can: (permission: Permission) => boolean;
  refresh: () => Promise<void>;
  signOut: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside <AuthProvider>");
  return ctx;
}

const ACTIVITY_EVENTS = ["pointerdown", "keydown", "wheel", "pointermove"] as const;
const HEARTBEAT_MS = 60_000;
const CHECK_MS = 10_000;

/**
 * Locks after `minutes` without keyboard/mouse activity, and tells the backend the
 * user is still active (at most once a minute) so its own idle check agrees.
 */
function useIdleLock(enabled: boolean, minutes: number, onLock: () => void) {
  const lastActivity = useRef(Date.now());
  const lastHeartbeat = useRef(Date.now());

  useEffect(() => {
    if (!enabled) return;
    lastActivity.current = Date.now();
    const mark = () => {
      lastActivity.current = Date.now();
    };
    for (const e of ACTIVITY_EVENTS) window.addEventListener(e, mark, { passive: true });

    const timer = window.setInterval(() => {
      const now = Date.now();
      if (now - lastActivity.current > minutes * 60_000) {
        void api.lockSession().finally(onLock);
        return;
      }
      if (lastActivity.current > lastHeartbeat.current && now - lastHeartbeat.current > HEARTBEAT_MS) {
        lastHeartbeat.current = now;
        void api.touchSession().catch(() => onLock());
      }
    }, CHECK_MS);

    return () => {
      for (const e of ACTIVITY_EVENTS) window.removeEventListener(e, mark);
      window.clearInterval(timer);
    };
  }, [enabled, minutes, onLock]);
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AuthStatus>();
  const [error, setError] = useState<ApiError>();

  const refresh = useCallback(async () => {
    try {
      const next = await api.authStatus();
      setPasswordPolicy(next.password_policy);
      setStatus(next);
      setError(undefined);
    } catch (e) {
      setError(e instanceof ApiError ? e : new ApiError("database", String(e)));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onChange = () => void refresh();
    window.addEventListener(AUTH_CHANGED_EVENT, onChange);
    return () => window.removeEventListener(AUTH_CHANGED_EVENT, onChange);
  }, [refresh]);

  const signOut = useCallback(async () => {
    await api.signOut().catch(() => {});
    await refresh();
  }, [refresh]);

  const lockNow = useCallback(() => void refresh(), [refresh]);
  const user = status?.user ?? null;
  const signedIn = !!user && !user.must_change_password;
  useIdleLock(signedIn && !status?.locked, status?.idle_lock_minutes ?? 15, lockNow);

  // The update notice shows over every screen, including sign-in.
  const notice = <UpdateNotice enabled={status?.update_checks ?? false} signedIn={signedIn} />;
  const withNotice = (screen: ReactNode) => (
    <>
      {screen}
      {notice}
    </>
  );

  if (error) return withNotice(<ConnectionErrorScreen message={error.message} onRetry={refresh} />);
  if (!status) return <SplashScreen />;
  if (!status.connected) {
    return withNotice(<ConnectionErrorScreen message={status.connection_error ?? "Unknown error"} onRetry={refresh} />);
  }
  if (status.setup_required) return withNotice(<SetupScreen onDone={refresh} />);
  if (!user) return withNotice(<SignInScreen onDone={refresh} />);
  if (user.must_change_password)
    return withNotice(<ChangePasswordScreen user={user} onDone={refresh} onSignOut={signOut} />);

  const value: AuthContextValue = {
    user,
    idleLockMinutes: status.idle_lock_minutes,
    can: (p) => roleCan(user.role, p),
    refresh,
    signOut,
  };

  return (
    <AuthContext.Provider value={value}>
      {/* The app stays mounted under the lock screen so unsaved form input survives. */}
      <div aria-hidden={status.locked} className={`h-full ${status.locked ? "pointer-events-none select-none" : ""}`}>
        {children}
      </div>
      {status.locked && <LockScreen user={user} onUnlocked={refresh} onSignOut={signOut} />}
      {notice}
    </AuthContext.Provider>
  );
}
