/*
 * dc-lite.js — a tiny renderer so the exported mockups run standalone.
 * It understands the subset of template syntax the mockups use:
 *   {{path}} holes in text and attributes, <sc-if value>, <sc-for list as>,
 *   data-for / data-as on an element (used for table rows), and onX="{{fn}}" events.
 * It is a prototype aid only — reimplement the pages in your own components.
 */
(function () {
  var HOLE = /\{\{\s*([^}]+?)\s*\}\}/g;

  function lookup(path, scope) {
    if (path === 'true') return true;
    if (path === 'false') return false;
    if (path === 'null') return null;
    if (/^-?\d+(\.\d+)?$/.test(path)) return Number(path);
    var parts = path.split('.');
    var v = scope[parts[0]];
    for (var i = 1; i < parts.length; i++) {
      if (v == null) return undefined;
      v = v[parts[i]];
    }
    return v;
  }
  function whole(str) {
    var m = (str || '').match(/^\s*\{\{\s*([^}]+?)\s*\}\}\s*$/);
    return m ? m[1] : null;
  }
  function interp(str, scope) {
    return str.replace(HOLE, function (_, p) {
      var v = lookup(p, scope);
      return v == null ? '' : String(v);
    });
  }
  function child(scope, as, item, i) {
    var s = Object.create(scope);
    s[as] = item;
    s.$index = i;
    return s;
  }

  function renderChildren(src, out, scope) {
    var nodes = Array.prototype.slice.call(src.childNodes);
    for (var i = 0; i < nodes.length; i++) renderNode(nodes[i], out, scope);
  }

  function renderNode(n, out, scope) {
    if (n.nodeType === 3) {
      out.appendChild(document.createTextNode(n.data.indexOf('{{') > -1 ? interp(n.data, scope) : n.data));
      return;
    }
    if (n.nodeType !== 1) return;
    var tag = n.localName.toLowerCase();
    if (tag === 'sc-if') {
      var p = whole(n.getAttribute('value'));
      if (p && lookup(p, scope)) renderChildren(n, out, scope);
      return;
    }
    if (tag === 'sc-for') {
      var lp = whole(n.getAttribute('list'));
      var list = (lp && lookup(lp, scope)) || [];
      var as = n.getAttribute('as') || 'item';
      list.forEach(function (item, i) { renderChildren(n, out, child(scope, as, item, i)); });
      return;
    }
    if (n.hasAttribute && n.hasAttribute('data-for')) {
      var fp = whole(n.getAttribute('data-for'));
      var rows = (fp && lookup(fp, scope)) || [];
      var ras = n.getAttribute('data-as') || 'item';
      rows.forEach(function (item, i) { renderEl(n, out, child(scope, ras, item, i)); });
      return;
    }
    renderEl(n, out, scope);
  }

  function renderEl(n, out, scope) {
    var el = document.createElementNS(n.namespaceURI, n.localName);
    var props = [];
    var attrs = Array.prototype.slice.call(n.attributes);
    for (var i = 0; i < attrs.length; i++) {
      var name = attrs[i].name, val = attrs[i].value;
      if (name.indexOf('hint-') === 0 || name === 'data-for' || name === 'data-as') continue;
      var p = whole(val);
      if (name.indexOf('on') === 0 && p) {
        var fn = lookup(p, scope);
        if (typeof fn === 'function') el.addEventListener(name.slice(2).toLowerCase(), fn);
        continue;
      }
      if (p) {
        var v = lookup(p, scope);
        if (name === 'checked' || name === 'disabled' || name === 'selected') {
          props.push([name, !!v]);
          if (v) el.setAttribute(name, '');
          continue;
        }
        if (name.indexOf('aria-') === 0) { el.setAttribute(name, String(v)); continue; }
        if (v === false || v == null) continue;
        el.setAttribute(name, v === true ? '' : String(v));
        continue;
      }
      el.setAttribute(name, val.indexOf('{{') > -1 ? interp(val, scope) : val);
    }
    renderChildren(n, el, scope);
    props.forEach(function (pr) { el[pr[0]] = pr[1]; });
    out.appendChild(el);
  }

  function DCLogic(props) {
    this.props = props || {};
    this.state = undefined;
  }
  DCLogic.prototype.setState = function (patch) {
    var next = typeof patch === 'function' ? patch(this.state, this.props) : patch;
    this.state = Object.assign({}, this.state || {}, next);
    this._render();
  };
  DCLogic.prototype.forceUpdate = function () { this._render(); };
  window.DCLogic = DCLogic;

  window.mountDC = function (Comp, props) {
    var tpl = document.getElementById('tpl');
    var root = document.getElementById('root');
    var c = new Comp(props);
    c._render = function () {
      var vals = c.renderVals();
      var frag = document.createDocumentFragment();
      renderChildren(tpl.content, frag, vals);
      var x = window.scrollX, y = window.scrollY;
      root.replaceChildren(frag);
      window.scrollTo(x, y);
    };
    c._render();
    if (c.componentDidMount) c.componentDidMount();
  };
})();
