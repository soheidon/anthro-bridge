import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { GatewayStatus, AsyncState } from "../types";

export function useHealthCheck(poll?: boolean): AsyncState<GatewayStatus> {
  const [data, setData] = useState<GatewayStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    invoke<GatewayStatus>("check_gateway_status")
      .then((result) => setData(result))
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!poll) return;
    const id = setInterval(refresh, 3000);
    return () => clearInterval(id);
  }, [poll, refresh]);

  return { data, error, loading, refresh };
}
