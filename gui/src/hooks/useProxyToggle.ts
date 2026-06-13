import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { StartProxyResult } from "../types";

export function useProxyToggle(): {
  managedRunning: boolean;
  loading: boolean;
  error: string | null;
  diag: string | null;
  successMessage: string | null;
  start: () => void;
  stop: () => void;
  clearDiag: () => void;
} {
  const [managedRunning, setManagedRunning] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [diag, setDiag] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);

  // Check status on mount
  useEffect(() => {
    invoke<boolean>("proxy_status")
      .then(setManagedRunning)
      .catch(() => setManagedRunning(false));
  }, []);

  // Poll every 3s while managedRunning
  useEffect(() => {
    if (!managedRunning) return;
    const id = setInterval(() => {
      invoke<boolean>("proxy_status")
        .then((alive) => {
          if (!alive) setManagedRunning(false);
        })
        .catch(() => setManagedRunning(false));
    }, 3000);
    return () => clearInterval(id);
  }, [managedRunning]);

  const start = useCallback(() => {
    setLoading(true);
    setError(null);
    setDiag(null);
    setSuccessMessage(null);
    invoke<StartProxyResult>("start_proxy")
      .then((result) => {
        setLoading(false);
        if (result.success) {
          setManagedRunning(true);
          // do not set diag or successMessage on success
        } else if (result.log === "already_running") {
          setManagedRunning(true);
          // do not set successMessage
        }
      })
      .catch((e: unknown) => {
        setLoading(false);
        const errMsg = String(e);
        setError(errMsg);
        setDiag(errMsg); // also show in diagnostics area
      });
  }, []);

  const stop = useCallback(() => {
    setLoading(true);
    setError(null);
    setDiag(null);
    setSuccessMessage(null);
    invoke<string>("stop_proxy")
      .then((_diagStr) => {
        setLoading(false);
        setManagedRunning(false);
        // do not set diag or successMessage on success
      })
      .catch((e: unknown) => {
        setLoading(false);
        const errMsg = String(e);
        setError(errMsg);
        setDiag(errMsg);
      });
  }, []);

  const clearDiag = useCallback(() => {
    setDiag(null);
    setSuccessMessage(null);
  }, []);

  return { managedRunning, loading, error, diag, successMessage, start, stop, clearDiag };
}
