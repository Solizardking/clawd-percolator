// clawd pageview beacon, self-hosted and vetted copy of clawd-launch pv.js.
// Sends only {site, path, ref}. Never reads localStorage, sessionStorage, cookies or the wallet seed,
// and loads no further code. Not used on the wallet page (readme.html).
(function () {
  try {
    var el = document.currentScript;
    var site = (el && el.dataset && el.dataset.site) || location.hostname.split('.')[0];
    var ref = '';
    try { if (document.referrer) ref = new URL(document.referrer).origin; } catch (e) {}
    var body = JSON.stringify({ site: String(site).slice(0, 40), path: location.pathname.slice(0, 200), ref: ref });
    var url = 'https://clawd-launch-production.up.railway.app/api/pv';
    if (navigator.sendBeacon) navigator.sendBeacon(url, new Blob([body], { type: 'text/plain' }));
    else fetch(url, { method: 'POST', body: body, keepalive: true, mode: 'no-cors', credentials: 'omit' });
  } catch (e) {}
})();
