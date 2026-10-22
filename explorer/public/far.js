(function () {
  var session = '';
  var busy = false;
  try { session = sessionStorage.getItem('far-session') || ''; } catch (e) { session = ''; }

  function $(id) { return document.getElementById(id); }
  function esc(value) {
    return String(value == null ? '' : value).replace(/[&<>"]/g, function (c) {
      return ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c];
    });
  }
  function notice(text) { var el = $('notice'); if (el) el.textContent = text; }
  function buttons() { return ['keys', 'shards', 'intent', 'mine', 'all'].map($); }

  function post(scene) {
    return fetch('/api/far', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ scene: scene, session: session }),
    }).then(function (response) { return response.json(); });
  }

  function paintBalances(parts) {
    var rows = [];
    var seen = Object.create(null);
    (parts || []).forEach(function (part) {
      Object.keys(part.balances || {}).forEach(function (id) {
        if (seen[id]) return;
        seen[id] = true;
        var row = part.balances[id];
        rows.push('<tr><td class="mono">' + esc(id) + '</td><td>' + esc(row.CLAWD) + '</td><td>' + esc(row.WIDGET) + '</td><td>' + esc(row.calls) + '</td></tr>');
      });
    });
    var body = $('balances');
    if (body) body.innerHTML = rows.length ? rows.join('') : '<tr><td class="sub" colspan="4">No accounts in this scene.</td></tr>';
  }

  function paintParts(parts) {
    var html = (parts || []).map(function (part) {
      var head = '<h3 style="margin:12px 0 6px">' + esc(part.scene) + (part.ok ? '' : ' · check the steps') + '</h3>';
      var steps = (part.steps || []).map(function (step) {
        var pill = step.ok ? '<span class="pill ok">pass</span>' : '<span class="pill bad">miss</span>';
        var why = step.reason ? ' <span class="mono">' + esc(step.reason) + '</span>' : '';
        return '<div class="step">' + pill + '<b class="mono">' + esc(step.name) + '</b>' + why + '<span>' + esc(step.detail) + '</span></div>';
      }).join('');
      var extra = '';
      if (part.extra && part.extra.timing) {
        var timing = part.extra.timing;
        extra += '<p class="sub">one shard ' + esc(timing.oneNs) + ' ns · two shards ' + esc(timing.twoNs) + ' ns</p>';
      }
      if (part.extra && part.extra.receipt) {
        extra += '<p class="sub">receipt payer ' + esc(part.extra.receipt.payer) + ' · left ' + esc(part.extra.receipt.left) + ' · right ' + esc(part.extra.receipt.right) + '</p>';
      }
      if (part.extra && part.extra.reward != null) {
        extra += '<p class="sub">mined ' + esc(part.extra.reward) + ' · confirmations ' + esc(part.extra.confirmations) + '</p>';
      }
      return head + steps + extra;
    }).join('');
    var log = $('log');
    if (log) log.innerHTML = html || '<span class="sub">No steps.</span>';
  }

  function paintCharges(parts) {
    var charges = parts && parts[0] && parts[0].charges;
    var el = $('charges');
    if (!el || !charges) return;
    el.textContent = Object.keys(charges).map(function (kind) { return kind + ' ' + charges[kind]; }).join(' · ');
  }

  function apply(body) {
    if (body && body.session) {
      session = body.session;
      try { sessionStorage.setItem('far-session', session); } catch (e) {}
    }
    paintParts(body && body.parts);
    paintBalances(body && body.parts);
    paintCharges(body && body.parts);
  }

  function run(scene) {
    if (busy) return;
    busy = true;
    buttons().forEach(function (button) { if (button) button.disabled = true; });
    notice(scene === 'shards' || scene === 'all' ? 'Measuring both shards…' : 'Running ' + scene + '…');
    post(scene).then(function (body) {
      if (body && body.reason === 'session') return post('boot').then(function (booted) { apply(booted); return post(scene); });
      return body;
    }).then(function (body) {
      apply(body);
      if (!body) notice('The demo request failed.');
      else if (body.ok === false && body.reason) notice('Stopped: ' + body.reason);
      else if (body.ok === false) notice('Finished. One step did not match. The log names it.');
      else if (scene === 'boot') notice('Session ready. Private keys stay on the server. Pick a scene.');
      else notice('Done. ' + scene + ' used the Far runtime on this explorer.');
    }).catch(function () {
      notice('The demo request failed.');
    }).then(function () {
      busy = false;
      buttons().forEach(function (button) { if (button) button.disabled = false; });
    });
  }

  ['keys', 'shards', 'intent', 'mine', 'all'].forEach(function (scene) {
    var button = $(scene);
    if (button) button.addEventListener('click', function () { run(scene); });
  });
  run('boot');
})();
