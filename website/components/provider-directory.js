/* ==========================================================================
   ProviderDirectory — Standalone UI Component Module
   RetinaX Design System: Clinical White · Slate Navy · Medical Teal

   Lists verified healthcare providers with specialty, status, search, and filtering.

   Usage (HTML attribute API):
   --------------------------------------------------------------------------
   <div class="provider-directory-mount"
        data-title="Verified Providers"
        data-page-size="6"
        data-providers='[...]'></div>
   --------------------------------------------------------------------------
   JS Programmatic API:
   --------------------------------------------------------------------------
   const dir = new ProviderDirectory(mountEl, {
     title: 'Provider Directory',
     pageSize: 6,
     providers: [ ... ],
     fetchProviders: async () => [ ... ]
   });
   ========================================================================== */

(function (root, factory) {
  if (typeof define === 'function' && define.amd) {
    define([], factory);
  } else if (typeof module === 'object' && module.exports) {
    module.exports = factory();
  } else {
    root.ProviderDirectory = factory();
  }
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  var DEFAULT_PAGE_SIZE = 6;
  var DEFAULT_TITLE = 'Provider Directory';

  function escHtml(value) {
    if (value === null || value === undefined) return '';
    return String(value)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  function parseArray(raw) {
    if (!raw) return [];
    if (Array.isArray(raw)) return raw;
    if (typeof raw === 'string') {
      try {
        var parsed = JSON.parse(raw);
        return Array.isArray(parsed) ? parsed : [];
      } catch (_) {
        return [];
      }
    }
    return [];
  }

  function truncateAddr(addr) {
    var s = String(addr || '');
    if (s.length <= 12) return s;
    return s.slice(0, 6) + '…' + s.slice(-4);
  }

  function filterProviders(all, search, specialty, status) {
    if (!Array.isArray(all)) return [];
    var q = String(search || '')
      .trim()
      .toLowerCase();
    var spec = String(specialty || '')
      .trim()
      .toLowerCase();
    var stat = String(status || '')
      .trim()
      .toLowerCase();

    return all.filter(function (p) {
      if (!p) return false;
      var name = String(p.name || '').toLowerCase();
      var clinic = String(p.clinic || '').toLowerCase();
      var loc = String(p.location || '').toLowerCase();
      var pSpec = String(p.specialty || '').toLowerCase();
      var addr = String(p.address || '').toLowerCase();
      var pStatus = String(p.status || (p.verified ? 'Verified' : 'Pending')).toLowerCase();

      if (
        q &&
        name.indexOf(q) === -1 &&
        clinic.indexOf(q) === -1 &&
        loc.indexOf(q) === -1 &&
        addr.indexOf(q) === -1
      ) {
        return false;
      }
      if (spec && spec !== 'all' && pSpec !== spec) {
        return false;
      }
      if (stat && stat !== 'all' && pStatus !== stat) {
        return false;
      }
      return true;
    });
  }

  function buildCardHTML(p) {
    var isVerified = p.verified === true || String(p.status).toLowerCase() === 'verified';
    var statusClass = isVerified
      ? 'provider-card__badge--verified'
      : 'provider-card__badge--pending';
    var statusText = isVerified ? 'Verified' : p.status || 'Pending';
    var rating = typeof p.rating === 'number' ? p.rating.toFixed(1) : p.rating || '5.0';
    var languages = Array.isArray(p.languages) ? p.languages.join(', ') : p.languages || 'English';

    return [
      '<article class="provider-card" data-provider-id="' + escHtml(p.id) + '">',
      '  <div class="provider-card__header">',
      '    <div class="provider-card__avatar" aria-hidden="true">' +
        escHtml((p.name || 'Dr').slice(0, 2).toUpperCase()) +
        '</div>',
      '    <div class="provider-card__headings">',
      '      <h3 class="provider-card__name">' + escHtml(p.name || 'Healthcare Provider') + '</h3>',
      '      <span class="provider-card__specialty">' +
        escHtml(p.specialty || 'Ophthalmology') +
        '</span>',
      '    </div>',
      '    <span class="provider-card__badge ' +
        statusClass +
        '">' +
        escHtml(statusText) +
        '</span>',
      '  </div>',
      '  <div class="provider-card__body">',
      p.clinic ? '    <p class="provider-card__clinic">🏥 ' + escHtml(p.clinic) + '</p>' : '',
      p.location ? '    <p class="provider-card__location">📍 ' + escHtml(p.location) + '</p>' : '',
      '    <p class="provider-card__address" title="' +
        escHtml(p.address) +
        '">🔑 ' +
        escHtml(truncateAddr(p.address)) +
        '</p>',
      '    <div class="provider-card__meta">',
      '      <span class="provider-card__rating">⭐ ' + escHtml(rating) + '</span>',
      '      <span class="provider-card__languages">🌐 ' + escHtml(languages) + '</span>',
      '    </div>',
      '  </div>',
      '</article>',
    ].join('\n');
  }

  function buildHTML(state) {
    var specialties = [
      'All',
      'Ophthalmology',
      'Optometry',
      'Retina Specialist',
      'Corneal Specialist',
      'Pediatric Ophthalmology',
      'Glaucoma Specialist',
    ];

    var html = [
      '<div class="provider-directory">',
      '  <div class="provider-directory__header">',
      '    <div>',
      '      <h2 class="provider-directory__title">' + escHtml(state.title) + '</h2>',
      '      <p class="provider-directory__subtitle">Verified healthcare providers in the RetinaX decentralized network.</p>',
      '    </div>',
      '    <span class="provider-directory__count" aria-live="polite">' +
        state.total +
        ' providers found</span>',
      '  </div>',
      '  <div class="provider-directory__filters" role="search" aria-label="Provider Search and Filters">',
      '    <div class="provider-directory__search-box">',
      '      <label for="provider-search-input" class="provider-directory__label">Search</label>',
      '      <input type="text" id="provider-search-input" class="provider-directory__input" data-filter="search" placeholder="Search by name, clinic, or location..." value="' +
        escHtml(state.search) +
        '" />',
      '    </div>',
      '    <div class="provider-directory__select-box">',
      '      <label for="provider-specialty-select" class="provider-directory__label">Specialty</label>',
      '      <select id="provider-specialty-select" class="provider-directory__select" data-filter="specialty">',
      specialties
        .map(function (s) {
          var sel = String(state.specialty).toLowerCase() === s.toLowerCase() ? ' selected' : '';
          return (
            '        <option value="' + escHtml(s) + '"' + sel + '>' + escHtml(s) + '</option>'
          );
        })
        .join('\n'),
      '      </select>',
      '    </div>',
      '    <div class="provider-directory__select-box">',
      '      <label for="provider-status-select" class="provider-directory__label">Status</label>',
      '      <select id="provider-status-select" class="provider-directory__select" data-filter="status">',
      '        <option value="All"' +
        (state.status === 'All' ? ' selected' : '') +
        '>All Statuses</option>',
      '        <option value="Verified"' +
        (state.status === 'Verified' ? ' selected' : '') +
        '>Verified Only</option>',
      '        <option value="Pending"' +
        (state.status === 'Pending' ? ' selected' : '') +
        '>Pending Verification</option>',
      '      </select>',
      '    </div>',
      '  </div>',
    ];

    if (state.error) {
      html.push(
        '<div class="provider-directory__alert" role="alert">',
        '  <span>' + escHtml(state.error) + '</span>',
        '</div>'
      );
    }

    if (state.loading) {
      html.push(
        '<div class="provider-directory__skeleton" aria-live="polite">',
        '  <div class="provider-directory__skeleton-card"></div>',
        '  <div class="provider-directory__skeleton-card"></div>',
        '  <div class="provider-directory__skeleton-card"></div>',
        '  <p>Loading verified providers...</p>',
        '</div>'
      );
    } else if (state.rows.length === 0) {
      html.push(
        '<div class="provider-directory__empty">',
        '  <p class="provider-directory__empty-text">No providers match your search criteria.</p>',
        '</div>'
      );
    } else {
      html.push(
        '<div class="provider-directory__grid" role="region" aria-label="Providers list">',
        state.rows.map(buildCardHTML).join('\n'),
        '</div>'
      );
    }

    if (state.pageCount > 1) {
      html.push(
        '<div class="provider-directory__pagination" role="navigation" aria-label="Provider pages">',
        '  <button type="button" class="provider-directory__page-btn" data-page-action="prev" ' +
          (state.page <= 1 ? 'disabled' : '') +
          ' aria-label="Previous page">Previous</button>',
        '  <span class="provider-directory__page-info">Page ' +
          state.page +
          ' of ' +
          state.pageCount +
          '</span>',
        '  <button type="button" class="provider-directory__page-btn" data-page-action="next" ' +
          (state.page >= state.pageCount ? 'disabled' : '') +
          ' aria-label="Next page">Next</button>',
        '</div>'
      );
    }

    html.push('</div>');
    return html.join('\n');
  }

  function ProviderDirectory(mountEl, options) {
    if (!mountEl || (typeof HTMLElement !== 'undefined' && !(mountEl instanceof HTMLElement))) {
      throw new TypeError('ProviderDirectory expects a valid DOM element mount.');
    }

    var opts = options || {};
    var ds = mountEl.dataset || {};

    this._mount = mountEl;
    this._cfg = {
      title: opts.title || ds.title || DEFAULT_TITLE,
      pageSize: Math.max(1, parseInt(opts.pageSize || ds.pageSize, 10) || DEFAULT_PAGE_SIZE),
      fetchProviders: typeof opts.fetchProviders === 'function' ? opts.fetchProviders : null,
    };

    var rawProviders = opts.providers !== undefined ? opts.providers : ds.providers;
    this._all = parseArray(rawProviders);
    this._search = '';
    this._specialty = 'All';
    this._status = 'All';
    this._page = 1;
    this._loading = false;
    this._error = '';

    this._initDOM();
    this._render();
  }

  ProviderDirectory.prototype._initDOM = function () {
    var self = this;
    var root = document.createElement('div');
    root.className = 'provider-directory-root';
    this._mount.appendChild(root);

    root.addEventListener('input', function (e) {
      if (e.target && e.target.getAttribute('data-filter') === 'search') {
        self.setSearchQuery(e.target.value);
      }
    });

    root.addEventListener('change', function (e) {
      if (!e.target) return;
      var filterType = e.target.getAttribute('data-filter');
      if (filterType === 'specialty') {
        self.setSpecialtyFilter(e.target.value);
      } else if (filterType === 'status') {
        self.setStatusFilter(e.target.value);
      }
    });

    root.addEventListener('click', function (e) {
      var target = e.target;
      if (!target) return;
      var btn = target.closest('[data-page-action]');
      if (btn && !btn.disabled) {
        var action = btn.getAttribute('data-page-action');
        if (action === 'prev') self.prevPage();
        else if (action === 'next') self.nextPage();
      }
    });

    this._root = root;
  };

  ProviderDirectory.prototype.getFilteredProviders = function () {
    return filterProviders(this._all, this._search, this._specialty, this._status);
  };

  ProviderDirectory.prototype.getPageCount = function () {
    var filtered = this.getFilteredProviders();
    return Math.max(1, Math.ceil(filtered.length / this._cfg.pageSize));
  };

  ProviderDirectory.prototype._render = function () {
    if (!this._root) return;

    var filtered = this.getFilteredProviders();
    var pageCount = Math.max(1, Math.ceil(filtered.length / this._cfg.pageSize));
    if (this._page > pageCount) this._page = pageCount;
    if (this._page < 1) this._page = 1;

    var start = (this._page - 1) * this._cfg.pageSize;
    var rows = filtered.slice(start, start + this._cfg.pageSize);

    this._root.innerHTML = buildHTML({
      title: this._cfg.title,
      rows: rows,
      page: this._page,
      pageCount: pageCount,
      loading: this._loading,
      error: this._error,
      search: this._search,
      specialty: this._specialty,
      status: this._status,
      total: filtered.length,
    });
  };

  ProviderDirectory.prototype.setProviders = function (providers) {
    this._all = parseArray(providers);
    this._page = 1;
    this._loading = false;
    this._error = '';
    this._render();
  };

  ProviderDirectory.prototype.setSearchQuery = function (query) {
    this._search = String(query || '');
    this._page = 1;
    this._render();
  };

  ProviderDirectory.prototype.setSpecialtyFilter = function (specialty) {
    this._specialty = String(specialty || 'All');
    this._page = 1;
    this._render();
  };

  ProviderDirectory.prototype.setStatusFilter = function (status) {
    this._status = String(status || 'All');
    this._page = 1;
    this._render();
  };

  ProviderDirectory.prototype.load = function () {
    var self = this;
    var provider = this._cfg.fetchProviders;
    if (typeof provider !== 'function') return Promise.resolve(this._all);

    this.showLoading();

    return Promise.resolve()
      .then(function () {
        return provider();
      })
      .then(function (rows) {
        self.setProviders(rows);
        return self._all;
      })
      .catch(function (err) {
        var msg = err && err.message ? err.message : 'Could not load provider directory.';
        self.showError(msg);
        return [];
      });
  };

  ProviderDirectory.prototype.showLoading = function () {
    this._loading = true;
    this._error = '';
    this._render();
  };

  ProviderDirectory.prototype.showError = function (msg) {
    this._loading = false;
    this._error = msg || 'An error occurred';
    this._render();
  };

  ProviderDirectory.prototype.goToPage = function (page) {
    var next = Math.min(Math.max(1, parseInt(page, 10) || 1), this.getPageCount());
    if (next === this._page) return;
    this._page = next;
    this._render();
  };

  ProviderDirectory.prototype.nextPage = function () {
    this.goToPage(this._page + 1);
  };

  ProviderDirectory.prototype.prevPage = function () {
    this.goToPage(this._page - 1);
  };

  ProviderDirectory.prototype.destroy = function () {
    if (this._root && this._root.parentNode) {
      this._root.parentNode.removeChild(this._root);
    }
    this._root = null;
    this._mount = null;
    this._all = [];
    this._cfg = null;
  };

  return ProviderDirectory;
});
