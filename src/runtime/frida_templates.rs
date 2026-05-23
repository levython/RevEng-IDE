//! Pre-built Frida script templates for common Android RE tasks.

pub struct ScriptTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub code: &'static str,
}

pub fn get_templates() -> &'static [ScriptTemplate] {
    &TEMPLATES
}

static TEMPLATES: &[ScriptTemplate] = &[
    ScriptTemplate {
        name: "SSL Pinning Bypass",
        description: "Bypasses OkHttp3, TrustManager, and HostnameVerifier certificate pinning",
        code: r#"// SSL Pinning Bypass — OkHttp3 + X509TrustManager + HostnameVerifier
Java.perform(function() {

  // Bypass OkHttp3 CertificatePinner
  try {
    var CertificatePinner = Java.use('okhttp3.CertificatePinner');
    CertificatePinner.check.overload('java.lang.String', 'java.util.List').implementation = function(host, certs) {
      console.log('[+] OkHttp3 CertificatePinner.check() bypassed -> ' + host);
    };
    CertificatePinner.check.overload('java.lang.String', 'java.security.cert.Certificate').implementation = function(host, cert) {
      console.log('[+] OkHttp3 CertificatePinner.check() bypassed -> ' + host);
    };
  } catch(e) { console.log('[-] OkHttp3 CertificatePinner not found.'); }

  // Bypass X509TrustManager — accept all certs
  try {
    var X509TrustManager = Java.use('javax.net.ssl.X509TrustManager');
    var SSLContext = Java.use('javax.net.ssl.SSLContext');
    var TrustAll = Java.registerClass({
      name: 'com.reveng.TrustAll',
      implements: [X509TrustManager],
      methods: {
        checkClientTrusted: function(chain, authType) {},
        checkServerTrusted: function(chain, authType) {},
        getAcceptedIssuers: function() { return []; }
      }
    });
    var sslCtx = SSLContext.getInstance('TLS');
    sslCtx.init(null, [TrustAll.$new()], null);
    var HttpsURLConnection = Java.use('javax.net.ssl.HttpsURLConnection');
    HttpsURLConnection.setDefaultSSLSocketFactory(sslCtx.getSocketFactory());
    console.log('[+] TrustAll X509TrustManager installed.');
  } catch(e) { console.log('[-] TrustManager bypass failed: ' + e); }

  // Bypass OkHttp3 HostnameVerifier
  try {
    var OkHostnameVerifier = Java.use('okhttp3.internal.tls.OkHostnameVerifier');
    OkHostnameVerifier.verify.overload('java.lang.String', 'javax.net.ssl.SSLSession').implementation = function(host, session) {
      console.log('[+] OkHostnameVerifier.verify() -> true for ' + host);
      return true;
    };
  } catch(e) {}

  console.log('[*] SSL pinning bypass active.');
});
"#,
    },
    ScriptTemplate {
        name: "Root Detection Bypass",
        description: "Bypasses RootBeer, su binary checks, and common root file checks",
        code: r#"// Root Detection Bypass
Java.perform(function() {

  // Bypass RootBeer
  try {
    var RootBeer = Java.use('com.scottyab.rootbeer.RootBeer');
    var methods = ['isRooted','isRootedWithBusyBoxCheck','detectTestKeys',
                   'checkForBusyBoxBinary','checkForSuBinary','checkSuExists',
                   'checkForRWPaths','checkDangerousProps','checkRootAccessGiven'];
    methods.forEach(function(m) {
      try {
        RootBeer[m].implementation = function() {
          console.log('[+] RootBeer.' + m + '() -> false');
          return false;
        };
      } catch(e) {}
    });
  } catch(e) { console.log('[-] RootBeer not found.'); }

  // Bypass Runtime.exec for su / busybox
  var Runtime = Java.use('java.lang.Runtime');
  Runtime.exec.overload('java.lang.String').implementation = function(cmd) {
    if (/\bsu\b|busybox|which/.test(cmd)) {
      console.log('[+] Blocked Runtime.exec: ' + cmd);
      throw Java.use('java.io.IOException').$new('No such file');
    }
    return this.exec(cmd);
  };

  // Bypass File.exists for root paths
  var File = Java.use('java.io.File');
  var ROOT_PATHS = ['/system/bin/su','/system/xbin/su','/sbin/su',
                    '/system/app/Superuser.apk','/system/app/SuperSU.apk'];
  File.exists.implementation = function() {
    var p = this.getAbsolutePath();
    if (ROOT_PATHS.indexOf(p) !== -1) {
      console.log('[+] File.exists blocked: ' + p);
      return false;
    }
    return this.exists();
  };

  console.log('[*] Root detection bypass active.');
});
"#,
    },
    ScriptTemplate {
        name: "Method Tracer",
        description: "Traces all method calls on a target Java class with args and return values",
        code: r#"// Method Tracer — set CLASS_NAME to the class you want to trace
var CLASS_NAME = 'com.example.app.MainActivity'; // <-- CHANGE THIS

Java.perform(function() {
  try {
    var Cls = Java.use(CLASS_NAME);
    var methods = Cls.class.getDeclaredMethods();
    methods.forEach(function(method) {
      var name = method.getName();
      try {
        Cls[name].overloads.forEach(function(overload) {
          overload.implementation = function() {
            var args = Array.prototype.slice.call(arguments)
              .map(function(a) { try { return JSON.stringify(a); } catch(e) { return String(a); } })
              .join(', ');
            console.log('[TRACE] >> ' + CLASS_NAME + '.' + name + '(' + args + ')');
            var ret = this[name].apply(this, arguments);
            console.log('[TRACE] << ' + name + ' = ' + JSON.stringify(ret));
            return ret;
          };
        });
      } catch(e) {}
    });
    console.log('[*] Tracing ' + methods.length + ' methods on ' + CLASS_NAME);
  } catch(e) {
    console.log('[-] Class not found: ' + CLASS_NAME + ' | ' + e);
  }
});
"#,
    },
    ScriptTemplate {
        name: "Anti-Debug Bypass",
        description: "Bypasses debugger detection, isDebuggerConnected, and emulator fingerprint checks",
        code: r#"// Anti-Debug / Anti-Emulator Bypass
Java.perform(function() {

  // Bypass Debug.isDebuggerConnected
  var Debug = Java.use('android.os.Debug');
  Debug.isDebuggerConnected.implementation = function() {
    console.log('[+] isDebuggerConnected() -> false');
    return false;
  };

  // Bypass Debug.waitingForDebugger
  Debug.waitingForDebugger.implementation = function() { return false; };

  // Spoof Build fields to look like a real Google Pixel
  try {
    var Build = Java.use('android.os.Build');
    Build.FINGERPRINT.value = 'google/walleye/walleye:9/PPR1.180610.009/4898911:user/release-keys';
    Build.MANUFACTURER.value = 'Google';
    Build.MODEL.value = 'Pixel 2';
    Build.BRAND.value = 'google';
    Build.PRODUCT.value = 'walleye';
    Build.DEVICE.value = 'walleye';
    Build.HARDWARE.value = 'walleye';
    console.log('[+] Build fields spoofed to Pixel 2.');
  } catch(e) { console.log('[-] Build spoof: ' + e); }

  // Bypass SystemProperties (ro.kernel.qemu, etc.)
  try {
    var SystemProperties = Java.use('android.os.SystemProperties');
    SystemProperties.get.overload('java.lang.String').implementation = function(key) {
      if (key === 'ro.kernel.qemu' || key === 'ro.kernel.android.qemud') return '0';
      return this.get(key);
    };
  } catch(e) {}

  console.log('[*] Anti-debug bypass active.');
});
"#,
    },
    ScriptTemplate {
        name: "Enumerate Loaded Classes",
        description: "Dumps all Java classes currently loaded in the process",
        code: r#"// Enumerate all loaded Java classes
Java.perform(function() {
  console.log('[*] Enumerating loaded classes...');
  var count = 0;
  Java.enumerateLoadedClasses({
    onMatch: function(name) {
      count++;
      console.log(name);
    },
    onComplete: function() {
      console.log('[*] Done. Total classes: ' + count);
    }
  });
});
"#,
    },
    ScriptTemplate {
        name: "Hook Native Export",
        description: "Intercepts a named export function in a .so library by name",
        code: r#"// Hook Native Export — edit MODULE and FUNC_NAME
var MODULE    = 'libnative.so';   // <-- CHANGE THIS
var FUNC_NAME = 'Java_com_example_app_NativeLib_check'; // <-- CHANGE THIS

var funcAddr = Module.findExportByName(MODULE, FUNC_NAME);
if (funcAddr) {
  Interceptor.attach(funcAddr, {
    onEnter: function(args) {
      console.log('[NATIVE] ' + FUNC_NAME + ' called');
      console.log('  arg0 (JNIEnv*): ' + args[0]);
      console.log('  arg1 (jobject): ' + args[1]);
      console.log('  arg2: ' + args[2]);
    },
    onLeave: function(retval) {
      console.log('  retval: ' + retval);
      // Patch return value: retval.replace(ptr(1));
    }
  });
  console.log('[+] Hooked ' + FUNC_NAME + ' @ ' + funcAddr);
} else {
  console.log('[-] Export not found: ' + FUNC_NAME);
  console.log('[*] Available exports in ' + MODULE + ':');
  try {
    Module.enumerateExports(MODULE).slice(0, 30).forEach(function(e) {
      console.log('    ' + e.name);
    });
  } catch(err) { console.log('    (could not enumerate: ' + err + ')'); }
}
"#,
    },
    ScriptTemplate {
        name: "Dump SharedPreferences",
        description: "Reads and dumps all SharedPreferences key-value pairs at runtime",
        code: r#"// Dump SharedPreferences
Java.perform(function() {
  var ActivityThread = Java.use('android.app.ActivityThread');
  var context = ActivityThread.currentApplication().getApplicationContext();
  var PreferenceManager = Java.use('android.preference.PreferenceManager');
  var prefs = PreferenceManager.getDefaultSharedPreferences(context);
  var all = prefs.getAll();
  var iter = all.keySet().iterator();
  var count = 0;
  console.log('[*] SharedPreferences dump:');
  while (iter.hasNext()) {
    var key = iter.next().toString();
    console.log('  ' + key + ' = ' + all.get(key));
    count++;
  }
  console.log('[*] Total entries: ' + count);
});
"#,
    },
    ScriptTemplate {
        name: "Intent Monitor",
        description: "Monitors all startActivity and sendBroadcast calls, logs intent data",
        code: r#"// Intent Monitor — logs all startActivity / sendBroadcast calls
Java.perform(function() {
  var Activity = Java.use('android.app.Activity');
  Activity.startActivity.overload('android.content.Intent').implementation = function(intent) {
    console.log('[INTENT] startActivity: ' + intent.toString());
    logIntentExtras(intent);
    return this.startActivity(intent);
  };

  var ContextWrapper = Java.use('android.content.ContextWrapper');
  ContextWrapper.sendBroadcast.overload('android.content.Intent').implementation = function(intent) {
    console.log('[INTENT] sendBroadcast: ' + intent.toString());
    logIntentExtras(intent);
    return this.sendBroadcast(intent);
  };

  function logIntentExtras(intent) {
    try {
      var extras = intent.getExtras();
      if (extras !== null) {
        var keys = extras.keySet().toArray();
        for (var i = 0; i < keys.length; i++) {
          console.log('  extra: ' + keys[i] + ' = ' + extras.get(keys[i]));
        }
      }
    } catch(e) {}
  }

  console.log('[*] Intent monitor active.');
});
"#,
    },
    ScriptTemplate {
        name: "Blank Script",
        description: "Empty starting template — write your hooks from scratch",
        code: r#"// Frida script — write your hooks here
Java.perform(function() {
  // Example: hook a method
  // var MyClass = Java.use('com.example.MyClass');
  // MyClass.someMethod.implementation = function(arg) {
  //   console.log('[*] someMethod called: ' + arg);
  //   return this.someMethod(arg); // call original
  // };

  console.log('[*] Script loaded.');
});
"#,
    },
];
