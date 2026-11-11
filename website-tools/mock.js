(() => {
  const base = { vncPort:null, sshPort:22, rdpPort:null, rdpDomain:"", rdpUser:"", rdpTarget:"", rdpSessionUser:"", desktopProtocol:"vnc", user:"", defaultView:"desktop", orientation:"landscape", sharedClipboard:true, playAudio:false, skipCertCheck:false, os:"", customThumb:false, credentialId:null, rdpCredentialId:null, vncCredentialId:null, gatewayCredentialId:null, rdpNoNtlm:false, rdpKdc:"", rdpGateway:"", rdpGatewayUser:"", sharedFolder:"", fitToWindow:true, hasPassword:true, hasVncPassword:false, hasRdpPassword:false, hasGatewayPassword:false, hasKey:false, fileTransfer:"auto", ftpPort:null, ftpSecurity:"explicit", ftpActive:false, filesVia:"ssh", sshLegacy:false, sshKeepalive:30, sshCompression:false, forwards:[], jumpHost:"", sshAgent:false, agentForward:false, lastUsed:null, mac:"", sshTunnel:false, persistShell:"" };
  const C = (o) => ({ ...base, ...o });
  const now = new Date();
  const ago = (h) => new Date(now - h * 3600e3).toISOString();
  window.__conns = [
    C({ id:"1", name:"Studio-Mac", group:"Büro", address:"studio-mac.local", vncPort:5900, sshPort:22, os:"macos", user:"anna", lastUsed:ago(1), hasKey:true }),
    C({ id:"2", name:"Buchhaltung", group:"Büro", address:"192.168.10.24", rdpPort:3389, sshPort:null, os:"windows", user:"buchhaltung", desktopProtocol:"rdp", lastUsed:ago(20) }),
    C({ id:"3", name:"Empfang", group:"Büro", address:"empfang.firma.local", rdpPort:3389, sshPort:null, os:"windows", user:"empfang", desktopProtocol:"rdp", lastUsed:ago(70) }),
    C({ id:"4", name:"Werkstatt-PC", group:"Werkstatt", address:"werkstatt.local", rdpPort:3389, vncPort:null, sshPort:22, os:"ubuntu", user:"lukas", desktopProtocol:"rdp", lastUsed:ago(5) }),
    C({ id:"5", name:"Raspberry Pi Wetterstation", group:"Werkstatt", address:"wetter-pi.local", vncPort:5900, sshPort:22, os:"raspberry", user:"pi", hasKey:true, lastUsed:ago(30) }),
    C({ id:"6", name:"Webserver", group:"Server", address:"web01.example.ch", vncPort:null, sshPort:22, os:"linux", user:"deploy", hasKey:true, hasPassword:false, defaultView:"shell", lastUsed:ago(2) }),
    C({ id:"7", name:"NAS", group:"Server", address:"nas.local", vncPort:null, sshPort:22, os:"linux", user:"admin", defaultView:"files", lastUsed:ago(48) }),
    C({ id:"8", name:"Datenbank", group:"Server", address:"db01.example.ch", vncPort:null, sshPort:2222, os:"linux", user:"deploy", hasKey:true, hasPassword:false, defaultView:"shell", lastUsed:ago(100) }),
  ];
  const groups = [{ name:"Büro", color:"green" }, { name:"Werkstatt", color:"orange" }, { name:"Server", color:"blue" }];
  const t = Date.now()/1000; const creds = [[{ id:"c1", name:"Deploy-Schlüssel", user:"deploy", hasPassword:false, hasKey:true, lastUsed:t-7200 }, ["Webserver","Datenbank"]], [{ id:"c2", name:"Büro-Konto Anna", user:"anna", hasPassword:true, hasKey:false, lastUsed:t-3600 }, ["Studio-Mac"]], [{ id:"c3", name:"Werkstatt", user:"lukas", hasPassword:true, hasKey:true, lastUsed:t-18000 }, ["Werkstatt-PC","Raspberry Pi Wetterstation"]], [{ id:"c4", name:"Windows-Domäne", user:"FIRMA\\empfang", hasPassword:true, hasKey:false, lastUsed:t-86400*3 }, ["Buchhaltung","Empfang"]]];
  const handlers = window.__mockHandlers = {
    library_list: () => window.__conns,
    groups_list: () => groups,
    creds_list: () => creds,
    conn_reach: (a) => ({ "1":4, "2":7, "3":null, "4":3, "5":12, "6":18, "7":2, "8":21 })[a.id] ?? null,
    term_profiles: () => [],
    local_home: () => "/Users/anna",
    perm_list: () => [],
    certs_list: () => [],
    "plugin:event|listen": () => Math.floor(Math.random()*1e6),
    "plugin:app|version": () => "26.11.1",
    "plugin:os|platform": () => "macos",
  };
  const I = window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow:{ label:"main" }, currentWebview:{ windowLabel:"main", label:"main" } },
    callbacks: new Map(), plugins: {},
    transformCallback(cb, once) { const id = Math.floor(Math.random()*2**31); I.callbacks.set(id, (d) => { if (once) I.callbacks.delete(id); cb && cb(d); }); return id; },
    unregisterCallback(id) { I.callbacks.delete(id); },
    runCallback(id, d) { I.callbacks.get(id)?.(d); },
    convertFileSrc: (p) => p,
    async invoke(cmd, args) {
      window.__calls = (window.__calls||[]); window.__calls.push(cmd);
      const h = handlers[cmd];
      if (h) return h(args || {});
      if (window.__pending && window.__pending[cmd]) return window.__pending[cmd](args);
      return null;
    },
  };
  window.__send = (ch, message, index) => I.callbacks.get(ch.id)?.({ message, index });
})();
(() => {
  const H = window.__mockHandlers;
  const enc = (s) => Array.from(new TextEncoder().encode(s));
  H.shell_open = (a) => { setTimeout(() => window.__send(a.onEvent, { kind:"data", data: enc(window.__shellText || "") }, 0), 300); return "s1"; };
  H.term_open = H.shell_open;
  H.shell_probe = () => ({ model:"", osId:"ubuntu", temp:null, undervoltNow:false, undervoltSinceBoot:false });
  H.shell_held_list = () => []; H.shell_door_list = () => [];
  const desk = (a) => {
    setTimeout(() => {
      const cv = window.__desktopCanvas(); const W = cv.width, Hh = cv.height;
      const px = cv.getContext("2d").getImageData(0, 0, W, Hh).data;
      const sz = new Uint8Array(5); sz[0]=1; new DataView(sz.buffer).setUint16(1,W); new DataView(sz.buffer).setUint16(3,Hh);
      window.__send(a.onEvent, sz.buffer, 0);
      const b = new Uint8Array(3 + 8 + px.length); const v = new DataView(b.buffer);
      b[0]=7; v.setUint16(1,1); v.setUint16(3,0); v.setUint16(5,0); v.setUint16(7,W); v.setUint16(9,Hh); b.set(px, 11);
      window.__send(a.onEvent, b.buffer, 1);
    }, 300);
    return "d1";
  };
  H.vnc_open = desk; H.rdp_open = desk; H.rdp_check = () => null;
  H.sftp_open = () => ({ sid:"f1", scp:false });
  const D = (n, s) => (Date.now()/1000 - s*86400);
  H.sftp_list = (a) => ({ path:"/home/pi/wetter", parent:"/home/pi", entries:[
    { name:"archiv", dir:true, size:0, modified:D(0,12), perm:0o755 },
    { name:"logs", dir:true, size:0, modified:D(0,1), perm:0o755 },
    { name:"messung.py", dir:false, size:8421, modified:D(0,3), perm:0o644 },
    { name:"sensoren.yaml", dir:false, size:1290, modified:D(0,20), perm:0o644 },
    { name:"wetter-2026-11.csv", dir:false, size:2482113, modified:D(0,0.1), perm:0o644 },
    { name:"wetter-2026-10.csv", dir:false, size:7911204, modified:D(0,11), perm:0o644 },
    { name:"dashboard.png", dir:false, size:418220, modified:D(0,2), perm:0o644 },
    { name:"README.md", dir:false, size:3112, modified:D(0,40), perm:0o644 },
  ]});
  H.local_list = (a) => ({ path:"/Users/anna/Dokumente/Wetter", parent:"/Users/anna/Dokumente", entries:[
    { name:"Auswertung 2026.numbers", dir:false, size:1840221, modified:D(0,1), perm:null },
    { name:"Fotos", dir:true, size:0, modified:D(0,5), perm:null },
    { name:"Sensor-Kalibrierung.pdf", dir:false, size:921331, modified:D(0,30), perm:null },
    { name:"wetter-2026-10.csv", dir:false, size:7911204, modified:D(0,10), perm:null },
    { name:"Notizen.txt", dir:false, size:2210, modified:D(0,3), perm:null },
  ]});
  H.local_home = () => "/Users/anna/Dokumente/Wetter";
})();
