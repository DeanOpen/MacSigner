import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { coalesceScans, scanning } from "../lib/scan";
import { api, type CertList, type Preferences, type Services, type Settings, type ServiceName } from "../ipc/api";

export const keys = {
  certs: ["certificates"] as const,
  tokens: ["token-devices"] as const,
  services: ["services"] as const,
  settings: ["settings"] as const,
  prefs: ["preferences"] as const,
  info: ["app-info"] as const,
  activity: ["activity"] as const,
};

export const scanCertificates = coalesceScans(() => scanning(api.listCertificates(true)));

/** Windows store first (instant), then a background token scan. */
export function useCertificates() {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: keys.certs,
    queryFn: async () => {
      const quick = await api.listCertificates(false);
      // Token probing can take seconds; refresh the list when it is done.
      scanCertificates().then((full) => {
        qc.setQueryData(keys.certs, full);
        qc.invalidateQueries({ queryKey: keys.tokens });
      }).catch(() => qc.invalidateQueries({ queryKey: keys.tokens }));
      return quick;
    },
    staleTime: Infinity,
  });
  const rescan = useMutation({
    mutationFn: () => scanCertificates(true),
    onSuccess: (data: CertList) => {
      qc.setQueryData(keys.certs, data);
      qc.invalidateQueries({ queryKey: keys.tokens });
    },
  });
  return { ...query, rescan };
}

/** USB tokens from the last scan; refreshed whenever a scan finishes. */
export function useTokenDevices() {
  return useQuery({ queryKey: keys.tokens, queryFn: api.tokenDevices, staleTime: Infinity });
}

export function useServices() {
  return useQuery({ queryKey: keys.services, queryFn: api.services, refetchInterval: 3000 });
}

export function useServiceMutation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, running }: { name: ServiceName; running: boolean }) => api.setService(name, running),
    onSuccess: (data: Services) => qc.setQueryData(keys.services, data),
  });
}

export function useSettings() {
  return useQuery({ queryKey: keys.settings, queryFn: api.getSettings });
}

export function useSaveSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (s: Settings) => api.saveSettings(s).then(() => s),
    onSuccess: (s) => qc.setQueryData(keys.settings, s),
  });
}

export function usePreferences() {
  return useQuery({ queryKey: keys.prefs, queryFn: api.getPreferences });
}

export function useSavePreferences() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (p: Preferences) => api.savePreferences(p).then(() => p),
    onSuccess: (p) => qc.setQueryData(keys.prefs, p),
  });
}

/** Polled while visible so calls from web pages show up live. */
export function useActivity() {
  return useQuery({ queryKey: keys.activity, queryFn: api.activity, refetchInterval: 2000 });
}

export function useClearActivity() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.clearActivity,
    onSuccess: () => qc.setQueryData(keys.activity, []),
  });
}

export function useAppInfo() {
  return useQuery({ queryKey: keys.info, queryFn: api.appInfo, staleTime: Infinity });
}
