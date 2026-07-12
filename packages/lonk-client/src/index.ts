/** A single response-header pair attached to a link's 303 redirect. */
export type HeaderPair = [name: string, value: string];

export interface CreateLinkRequest {
  url: string;
  headers?: HeaderPair[];
}

export interface LinkResponse {
  id: string;
  url: string;
  short_url: string;
  qr_url: string;
  headers: HeaderPair[];
}

export interface ValidResponse {
  valid: boolean;
  error?: string;
}

export interface StatusResponse {
  id: string;
  url: string;
  alive: boolean;
  http_status?: number;
  error?: string;
}

/** Thrown for transport failures (no `status`) and non-2xx responses (with `status`). */
export class LonkError extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
    this.name = 'LonkError';
  }
}

export interface LonkClient {
  createLink(req: CreateLinkRequest): Promise<LinkResponse>;
  validateUrl(url: string): Promise<ValidResponse>;
  linkStatus(id: string): Promise<StatusResponse>;
  /** `${base}/${id}` — the short link itself. */
  shortUrl(id: string): string;
  /** `${base}/${id}/qr` — the SVG QR code. */
  qrUrl(id: string): string;
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(url, init);
  } catch {
    throw new LonkError('request failed - is the server up?');
  }
  const body = await res.json().catch(() => undefined);
  if (!res.ok) {
    const message =
      body && typeof body.error === 'string'
        ? body.error
        : `server returned ${res.status}`;
    throw new LonkError(message, res.status);
  }
  return body as T;
}

const JSON_POST = { method: 'POST', headers: { 'Content-Type': 'application/json' } };

/**
 * Create a client for a lonk server. `baseUrl` defaults to "" (same-origin),
 * which is correct both for the bundled UI and for bespoke UIs served via
 * LONK_WEB_DIR. Pass e.g. "https://s.example.com" for cross-origin use.
 */
export function createClient(baseUrl = ''): LonkClient {
  const base = baseUrl.replace(/\/+$/, '');
  return {
    createLink: (req) =>
      request<LinkResponse>(`${base}/api/links`, { ...JSON_POST, body: JSON.stringify(req) }),
    validateUrl: (url) =>
      request<ValidResponse>(`${base}/api/valid`, { ...JSON_POST, body: JSON.stringify({ url }) }),
    linkStatus: (id) => request<StatusResponse>(`${base}/${id}/status`),
    shortUrl: (id) => `${base}/${id}`,
    qrUrl: (id) => `${base}/${id}/qr`,
  };
}
