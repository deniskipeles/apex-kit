declare global {
  interface Window {
    __APEXKIT_ROOT_DOMAIN__?: string;
    __APEX_SCOPE__?: { type: 'root' | 'tenant' | 'sandbox'; id: string };
  }
}

export const getSubdomainTenant = (): string | null => {
  if (typeof window === 'undefined') return null;

  const host = window.location.hostname.toLowerCase();
  const rootDomain = (window.__APEXKIT_ROOT_DOMAIN__ || '').toLowerCase().trim();

  // If no root domain is configured or we are on the root domain itself
  if (!rootDomain || host === rootDomain) {
    return null;
  }

  // Exact match for subdomains: e.g. "apexkit-drive.kipeles.dev"
  const suffix = `.${rootDomain}`;
  if (host.endsWith(suffix)) {
    const sub = host.slice(0, -suffix.length);
    if (sub && !['www', 'api', 'admin', 'app'].includes(sub)) {
      return sub;
    }
  }

  return null;
};
