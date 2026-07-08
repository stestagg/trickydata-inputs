// Progressive enhancement for the trickydata report. No dependencies; the site
// is fully readable without JS — this only adds filtering, sorting, and tabs.
(function () {
  "use strict";

  // --- Payload tabs (input pages) ----------------------------------------
  document.querySelectorAll(".payload").forEach(function (section) {
    var tabs = Array.prototype.slice.call(section.querySelectorAll(".tab"));
    var panels = Array.prototype.slice.call(section.querySelectorAll(".panel"));

    function select(name) {
      tabs.forEach(function (t) {
        t.classList.toggle("active", t.dataset.tab === name);
      });
      panels.forEach(function (p) {
        p.classList.toggle("active", p.dataset.panel === name);
      });
    }

    tabs.forEach(function (tab) {
      tab.addEventListener("click", function () {
        select(tab.dataset.tab);
      });
    });

    select(section.dataset.defaultTab || (tabs[0] && tabs[0].dataset.tab));
  });

  // --- Inputs table: filter + sort ---------------------------------------
  var table = document.getElementById("inputs-table");
  if (!table) return;

  var tbody = table.querySelector("tbody");
  var rows = Array.prototype.slice.call(tbody.querySelectorAll("tr"));
  var counter = document.getElementById("row-count");

  // Filter rows by a space-separated substring match against data-haystack.
  var filter = document.getElementById("filter");
  if (filter) {
    filter.addEventListener("input", function () {
      var terms = filter.value.toLowerCase().split(/\s+/).filter(Boolean);
      var shown = 0;
      rows.forEach(function (row) {
        var hay = row.dataset.haystack.toLowerCase();
        var match = terms.every(function (t) {
          return hay.indexOf(t) !== -1;
        });
        row.hidden = !match;
        if (match) shown++;
      });
      if (counter) counter.textContent = shown + (shown === 1 ? " input" : " inputs");
    });
  }

  // Click a sortable header to sort; click again to reverse.
  var sortState = { key: null, dir: 1 };
  table.querySelectorAll("th.sortable").forEach(function (th) {
    th.addEventListener("click", function () {
      var key = th.dataset.sort;
      var numeric = th.dataset.type === "number";
      sortState.dir = sortState.key === key ? -sortState.dir : 1;
      sortState.key = key;

      var sorted = rows.slice().sort(function (a, b) {
        var av = a.getAttribute("data-" + key) || "";
        var bv = b.getAttribute("data-" + key) || "";
        var cmp = numeric
          ? Number(av) - Number(bv)
          : av.localeCompare(bv);
        return cmp * sortState.dir;
      });

      table.querySelectorAll("th").forEach(function (h) {
        h.classList.remove("sorted-asc", "sorted-desc");
      });
      th.classList.add(sortState.dir === 1 ? "sorted-asc" : "sorted-desc");

      sorted.forEach(function (row) {
        tbody.appendChild(row);
      });
    });
  });
})();
