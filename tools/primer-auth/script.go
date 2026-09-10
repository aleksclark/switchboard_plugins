package main

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"strings"
	"time"
)

func browserScript(cfg config, request browserRequest, expiry time.Time) (string, error) {
	var id [32]byte
	if _, err := rand.Read(id[:]); err != nil {
		return "", err
	}
	return browserScriptPayload(cfg, request, hex.EncodeToString(id[:]), expiry.UnixMilli())
}

func browserScriptPayload(cfg config, request browserRequest, requestID string, expiresAt int64) (string, error) {
	payload, err := json.Marshal(struct {
		RequestID string         `json:"requestID"`
		ExpiresAt int64          `json:"expiresAt"`
		UserID    string         `json:"userId"`
		Email     string         `json:"email"`
		Tenant    string         `json:"tenant"`
		Subject   string         `json:"subject"`
		Request   browserRequest `json:"request"`
		BodyText  string         `json:"bodyText,omitempty"`
	}{requestID, expiresAt, cfg.ClerkUserID, cfg.Email, cfg.TenantID, cfg.ParentSubject, request, string(request.Body)})
	if err != nil {
		return "", err
	}
	payloadLiteral, err := json.Marshal(string(payload))
	if err != nil {
		return "", err
	}
	return strings.Replace(browserJavaScript, "__PAYLOAD__", string(payloadLiteral), 1), nil
}

const browserJavaScript = `(async () => {
    const input = JSON.parse(__PAYLOAD__);
    const request = input.request;
    let writeStarted = false;
    let token = null;
    const lockKey = Symbol.for('primer-auth.in-flight');
    const owner = {};
    const deadline = input.expiresAt;
    const replayKey = Symbol.for('primer-auth.requests');
    let entry = null;
    const fail = () => ({status: 502, body: {error: writeStarted
        ? 'Browser request failed; write outcome unknown. Check state before any manual retry.'
        : 'Browser session or upstream request unavailable.'}});
    try {
        const correctLocation = () => Date.now() < deadline && location.origin === 'https://api.primerlms.com' && location.pathname.startsWith('/tasks/');
        const now = Date.now();
        if (!Number.isSafeInteger(deadline) || deadline <= now || deadline > now + 28000 ||
            typeof input.requestID !== 'string' || !/^[a-f0-9]{64}$/.test(input.requestID) || !correctLocation()) return fail();
        const seen = window[replayKey] || (window[replayKey] = new Map());
        for (const [id, state] of seen) {
            if (state.retainUntil <= now) seen.delete(id);
        }
        if (seen.has(input.requestID)) return {status: 409, body: {error: 'Browser request already used.'}};
        if (seen.size >= 1024) return {status: 503, body: {error: 'Browser operation still in progress.'}};
        entry = {state: 'started', retainUntil: now + 60000};
        seen.set(input.requestID, entry);
        if (window[lockKey]) return {status: 503, body: {error: 'Browser operation still in progress.'}};
        window[lockKey] = owner;
        const clerk = window.Clerk;
        if (!clerk?.loaded || !clerk.session || !clerk.user) return fail();
        const session = clerk.session;
        const sessionId = session.id;
        const identityMatches = () => correctLocation() && window.Clerk === clerk && clerk.loaded === true &&
            clerk.session === session && typeof sessionId === 'string' && sessionId.length > 0 &&
            clerk.session.id === sessionId && session.status === 'active' && session.user?.id === input.userId &&
            clerk.user?.id === input.userId &&
            clerk.user?.primaryEmailAddress?.emailAddress === input.email;
        if (!identityMatches()) return fail();
        let timer;
        try {
            token = await Promise.race([
                session.getToken(),
                new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('unavailable')), 2000); })
            ]);
        } finally {
            clearTimeout(timer);
        }
        if (!identityMatches() || typeof token !== 'string' || token.length === 0 || token.length > 65536) return fail();
        const readJSON = async (response, maximum) => {
            const type = (response.headers.get('content-type') || '').split(';')[0].trim().toLowerCase();
            if (type !== 'application/json' && !/^application\/[a-z0-9!#$&^_.+-]+\+json$/.test(type)) throw new Error('unavailable');
            const declared = response.headers.get('content-length');
            if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > maximum)) throw new Error('unavailable');
            if (!response.body) throw new Error('unavailable');
            const reader = response.body.getReader();
            const decoder = new TextDecoder('utf-8', {fatal: true});
            let size = 0;
            let text = '';
            let complete = false;
            try {
                for (;;) {
                    const {done, value} = await reader.read();
                    if (done) { complete = true; break; }
                    size += value.byteLength;
                    if (size > maximum) throw new Error('unavailable');
                    text += decoder.decode(value, {stream: true});
                }
                text += decoder.decode();
                return JSON.parse(text, (_, value) => {
                    if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))) throw new Error('unavailable');
                    return value;
                });
            } finally {
                if (!complete) await reader.cancel().catch(() => {});
                reader.releaseLock();
            }
        };
        const verification = await fetch('https://api.primerlms.com/tasks/api/auth/session', {
            method: 'GET', headers: {Authorization: 'Bearer ' + token, Accept: 'application/json'},
            credentials: 'omit', redirect: 'error', mode: 'same-origin', cache: 'no-store', signal: AbortSignal.timeout(12000)
        });
        if (verification.status !== 200) return fail();
        const verified = await readJSON(verification, 65536);
        if (!verified || Array.isArray(verified) || verified.tenantId !== input.tenant ||
            verified.subjectRef !== input.subject || !identityMatches()) return fail();
        const options = {
            method: request.method, headers: {Authorization: 'Bearer ' + token, Accept: 'application/json'},
            credentials: 'omit', redirect: 'error', mode: 'same-origin', cache: 'no-store', signal: AbortSignal.timeout(12000)
        };
        if (Object.hasOwn(request, 'body')) {
            options.headers['Content-Type'] = 'application/json';
            options.body = input.bodyText;
        }
        if (!identityMatches()) return fail();
        writeStarted = request.method !== 'GET';
        const response = await fetch('https://api.primerlms.com/tasks/api' + request.path, options);
        if (response.status < 200 || response.status > 599 || (response.status >= 300 && response.status < 400)) return fail();
        const body = response.status === 204 ? {status: 'success'} : await readJSON(response, 1048576);
        if (!identityMatches()) return fail();
        const serialized = JSON.stringify({status: response.status, body});
        if (new TextEncoder().encode(serialized).byteLength > 1572864 || serialized.includes(token)) return fail();
        return {status: response.status, body};
    } catch (_) {
        return fail();
    } finally {
        token = null;
        if (entry) entry.state = 'completed';
        if (window[lockKey] === owner) delete window[lockKey];
    }
})()`
