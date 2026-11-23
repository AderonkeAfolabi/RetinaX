/* ==========================================================================
   AppointmentScheduling — Standalone UI Component Module
   RetinaX Design System: Clinical White · Slate Navy · Medical Teal

   Follows docs/appointment-scheduling.md for appointment lifecycle:
     - Types: Examination, Consultation, FollowUp, Surgery, Emergency, Routine
     - Status: Scheduled, Confirmed, Completed, Cancelled, Rescheduled
     - Operations: schedule, confirm, reschedule, cancel, paginate, filter

   Usage (HTML attribute API):
   --------------------------------------------------------------------------
   <div class="appointment-scheduling-mount"
        data-title="Appointment Scheduling"
        data-page-size="5"
        data-appointments='[...]'></div>
   --------------------------------------------------------------------------
   JS Programmatic API:
   --------------------------------------------------------------------------
   const scheduler = new AppointmentScheduling(mountEl, {
     title: 'Patient Appointments',
     pageSize: 5,
     appointments: [ ... ],
     fetchAppointments: async () => [ ... ],
     onSchedule: async (data) => { ... },
     onStatusChange: async (id, newStatus) => { ... }
   });
   ========================================================================== */

(function (root, factory) {
  if (typeof define === 'function' && define.amd) {
    define([], factory);
  } else if (typeof module === 'object' && module.exports) {
    module.exports = factory();
  } else {
    root.AppointmentScheduling = factory();
  }
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  var DEFAULT_PAGE_SIZE = 5;
  var DEFAULT_TITLE = 'Appointment Scheduling';

  var APPOINTMENT_TYPES = {
    examination: { label: 'Examination', cls: 'appointment__type--exam' },
    consultation: { label: 'Consultation', cls: 'appointment__type--consult' },
    followup: { label: 'Follow-Up', cls: 'appointment__type--followup' },
    surgery: { label: 'Surgery', cls: 'appointment__type--surgery' },
    emergency: { label: 'Emergency', cls: 'appointment__type--emergency' },
    routine: { label: 'Routine Checkup', cls: 'appointment__type--routine' },
  };

  var STATUS_TYPES = {
    scheduled: { label: 'Scheduled', cls: 'appointment__status--scheduled' },
    confirmed: { label: 'Confirmed', cls: 'appointment__status--confirmed' },
    completed: { label: 'Completed', cls: 'appointment__status--completed' },
    cancelled: { label: 'Cancelled', cls: 'appointment__status--cancelled' },
    rescheduled: { label: 'Rescheduled', cls: 'appointment__status--rescheduled' },
  };

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

  function formatTime(timestamp) {
    if (!timestamp) return '—';
    var d;
    if (typeof timestamp === 'number') {
      d = new Date(timestamp > 1e11 ? timestamp : timestamp * 1000);
    } else {
      d = new Date(timestamp);
    }
    if (isNaN(d.getTime())) return String(timestamp);
    return d.toISOString().replace('T', ' ').substring(0, 16) + ' UTC';
  }

  function normalizeType(type) {
    var raw = String(type || '')
      .replace(/[\s_-]+/g, '')
      .toLowerCase();
    return (
      APPOINTMENT_TYPES[raw] || { label: type || 'Examination', cls: 'appointment__type--exam' }
    );
  }

  function normalizeStatus(status) {
    var raw = String(status || '')
      .replace(/[\s_-]+/g, '')
      .toLowerCase();
    return (
      STATUS_TYPES[raw] || { label: status || 'Scheduled', cls: 'appointment__status--scheduled' }
    );
  }

  function truncateAddr(addr) {
    var s = String(addr || '');
    if (s.length <= 12) return s;
    return s.slice(0, 6) + '…' + s.slice(-4);
  }

  function buildRowHTML(apt) {
    var typeMeta = normalizeType(apt.appointment_type || apt.type);
    var statusMeta = normalizeStatus(apt.status);
    var isActionable = apt.status !== 'Cancelled' && apt.status !== 'Completed';

    return [
      '<tr class="appointment-table__row" data-id="' + escHtml(apt.id) + '">',
      '  <td class="appointment-table__cell appointment-table__cell--id">#' +
        escHtml(apt.id) +
        '</td>',
      '  <td class="appointment-table__cell">',
      '    <span class="appointment__badge ' +
        typeMeta.cls +
        '">' +
        escHtml(typeMeta.label) +
        '</span>',
      '  </td>',
      '  <td class="appointment-table__cell" title="' +
        escHtml(apt.patient) +
        '">' +
        escHtml(truncateAddr(apt.patient)) +
        '</td>',
      '  <td class="appointment-table__cell" title="' +
        escHtml(apt.provider) +
        '">' +
        escHtml(truncateAddr(apt.provider)) +
        '</td>',
      '  <td class="appointment-table__cell">' + escHtml(formatTime(apt.scheduled_at)) + '</td>',
      '  <td class="appointment-table__cell">' + escHtml(apt.duration_minutes || 30) + ' min</td>',
      '  <td class="appointment-table__cell">',
      '    <span class="appointment__badge ' +
        statusMeta.cls +
        '">' +
        escHtml(statusMeta.label) +
        '</span>',
      '  </td>',
      '  <td class="appointment-table__cell appointment-table__cell--actions">',
      isActionable
        ? [
            '<button type="button" class="appointment__action-btn appointment__action-btn--confirm" data-apt-action="confirm" data-apt-id="' +
              escHtml(apt.id) +
              '" aria-label="Confirm appointment #' +
              escHtml(apt.id) +
              '">Confirm</button>',
            '<button type="button" class="appointment__action-btn appointment__action-btn--cancel" data-apt-action="cancel" data-apt-id="' +
              escHtml(apt.id) +
              '" aria-label="Cancel appointment #' +
              escHtml(apt.id) +
              '">Cancel</button>',
          ].join(' ')
        : '<span class="appointment__no-actions">—</span>',
      '  </td>',
      '</tr>',
    ].join('\n');
  }

  function buildHTML(state) {
    var html = [
      '<div class="appointment-scheduling">',
      '  <div class="appointment-scheduling__header">',
      '    <div class="appointment-scheduling__headings">',
      '      <h2 class="appointment-scheduling__title">' + escHtml(state.title) + '</h2>',
      '      <p class="appointment-scheduling__subtitle">Track and schedule verified clinical eye appointments.</p>',
      '    </div>',
      '    <div class="appointment-scheduling__header-actions">',
      '      <button type="button" class="appointment__btn appointment__btn--primary" data-action="toggle-form" aria-expanded="' +
        (state.showForm ? 'true' : 'false') +
        '">',
      state.showForm ? 'Close Form' : '+ Request Appointment',
      '      </button>',
      '    </div>',
      '  </div>',
    ];

    if (state.error) {
      html.push(
        '<div class="appointment__alert appointment__alert--error" role="alert">',
        '  <span>' + escHtml(state.error) + '</span>',
        '</div>'
      );
    }

    if (state.showForm) {
      html.push(
        '<form class="appointment-form" data-appointment-form>',
        '  <h3 class="appointment-form__title">Schedule New Appointment</h3>',
        '  <div class="appointment-form__grid">',
        '    <div class="appointment-form__group">',
        '      <label for="apt-patient" class="appointment-form__label">Patient Address</label>',
        '      <input type="text" id="apt-patient" name="patient" class="appointment-form__input" placeholder="G..." required />',
        '    </div>',
        '    <div class="appointment-form__group">',
        '      <label for="apt-provider" class="appointment-form__label">Provider Address</label>',
        '      <input type="text" id="apt-provider" name="provider" class="appointment-form__input" placeholder="G..." required />',
        '    </div>',
        '    <div class="appointment-form__group">',
        '      <label for="apt-type" class="appointment-form__label">Appointment Type</label>',
        '      <select id="apt-type" name="appointment_type" class="appointment-form__select">',
        '        <option value="Examination">Examination</option>',
        '        <option value="Consultation">Consultation</option>',
        '        <option value="FollowUp">Follow-Up</option>',
        '        <option value="Surgery">Surgery</option>',
        '        <option value="Emergency">Emergency</option>',
        '        <option value="Routine">Routine Checkup</option>',
        '      </select>',
        '    </div>',
        '    <div class="appointment-form__group">',
        '      <label for="apt-datetime" class="appointment-form__label">Scheduled Date & Time (UTC)</label>',
        '      <input type="datetime-local" id="apt-datetime" name="scheduled_at" class="appointment-form__input" required />',
        '    </div>',
        '    <div class="appointment-form__group">',
        '      <label for="apt-duration" class="appointment-form__label">Duration (Minutes)</label>',
        '      <input type="number" id="apt-duration" name="duration_minutes" class="appointment-form__input" value="30" min="5" max="480" />',
        '    </div>',
        '    <div class="appointment-form__group">',
        '      <label for="apt-notes" class="appointment-form__label">Clinical Notes</label>',
        '      <input type="text" id="apt-notes" name="notes" class="appointment-form__input" placeholder="Optional notes" />',
        '    </div>',
        '  </div>',
        '  <div class="appointment-form__actions">',
        '    <button type="submit" class="appointment__btn appointment__btn--submit">Confirm & Book</button>',
        '    <button type="button" class="appointment__btn appointment__btn--secondary" data-action="toggle-form">Cancel</button>',
        '  </div>',
        '</form>'
      );
    }

    if (state.loading) {
      html.push(
        '<div class="appointment-skeleton" aria-live="polite">',
        '  <div class="appointment-skeleton__line"></div>',
        '  <div class="appointment-skeleton__line"></div>',
        '  <div class="appointment-skeleton__line"></div>',
        '  <p class="appointment-skeleton__text">Loading appointments...</p>',
        '</div>'
      );
    } else if (state.rows.length === 0) {
      html.push(
        '<div class="appointment-empty">',
        '  <p class="appointment-empty__text">No appointments found.</p>',
        '</div>'
      );
    } else {
      html.push(
        '<div class="appointment-table-wrapper" role="region" aria-label="' +
          escHtml(state.title) +
          ' table" tabindex="0">',
        '  <table class="appointment-table">',
        '    <thead>',
        '      <tr>',
        '        <th scope="col" class="appointment-table__th">ID</th>',
        '        <th scope="col" class="appointment-table__th">Type</th>',
        '        <th scope="col" class="appointment-table__th">Patient</th>',
        '        <th scope="col" class="appointment-table__th">Provider</th>',
        '        <th scope="col" class="appointment-table__th">Scheduled (UTC)</th>',
        '        <th scope="col" class="appointment-table__th">Duration</th>',
        '        <th scope="col" class="appointment-table__th">Status</th>',
        '        <th scope="col" class="appointment-table__th">Actions</th>',
        '      </tr>',
        '    </thead>',
        '    <tbody>',
        state.rows.map(buildRowHTML).join('\n'),
        '    </tbody>',
        '  </table>',
        '</div>'
      );
    }

    if (state.pageCount > 1) {
      html.push(
        '<div class="appointment__pagination" role="navigation" aria-label="Appointment pages">',
        '  <button type="button" class="appointment__page-btn" data-page-action="prev" ' +
          (state.page <= 1 ? 'disabled' : '') +
          ' aria-label="Previous page">Previous</button>',
        '  <span class="appointment__page-info">Page ' +
          state.page +
          ' of ' +
          state.pageCount +
          '</span>',
        '  <button type="button" class="appointment__page-btn" data-page-action="next" ' +
          (state.page >= state.pageCount ? 'disabled' : '') +
          ' aria-label="Next page">Next</button>',
        '</div>'
      );
    }

    html.push('</div>');
    return html.join('\n');
  }

  function AppointmentScheduling(mountEl, options) {
    if (!mountEl || (typeof HTMLElement !== 'undefined' && !(mountEl instanceof HTMLElement))) {
      throw new TypeError('AppointmentScheduling expects a valid DOM element mount.');
    }

    var opts = options || {};
    var ds = mountEl.dataset || {};

    this._mount = mountEl;
    this._cfg = {
      title: opts.title || ds.title || DEFAULT_TITLE,
      pageSize: Math.max(1, parseInt(opts.pageSize || ds.pageSize, 10) || DEFAULT_PAGE_SIZE),
      fetchAppointments:
        typeof opts.fetchAppointments === 'function' ? opts.fetchAppointments : null,
      onSchedule: typeof opts.onSchedule === 'function' ? opts.onSchedule : null,
      onStatusChange: typeof opts.onStatusChange === 'function' ? opts.onStatusChange : null,
    };

    var rawApts = opts.appointments !== undefined ? opts.appointments : ds.appointments;
    this._all = parseArray(rawApts);
    this._page = 1;
    this._loading = false;
    this._error = '';
    this._showForm = false;

    this._initDOM();
    this._render();
  }

  AppointmentScheduling.prototype._initDOM = function () {
    var self = this;
    var root = document.createElement('div');
    root.className = 'appointment-scheduling-root';
    this._mount.appendChild(root);

    root.addEventListener('click', function (e) {
      var target = e.target;
      if (!target) return;

      var toggleBtn = target.closest('[data-action="toggle-form"]');
      if (toggleBtn) {
        self._showForm = !self._showForm;
        self._render();
        return;
      }

      var actionBtn = target.closest('[data-apt-action]');
      if (actionBtn && !actionBtn.disabled) {
        var action = actionBtn.getAttribute('data-apt-action');
        var id = actionBtn.getAttribute('data-apt-id');
        if (action === 'confirm') {
          self.confirmAppointment(id);
        } else if (action === 'cancel') {
          self.cancelAppointment(id);
        }
        return;
      }

      var pageBtn = target.closest('[data-page-action]');
      if (pageBtn && !pageBtn.disabled) {
        var pAction = pageBtn.getAttribute('data-page-action');
        if (pAction === 'prev') self.prevPage();
        else if (pAction === 'next') self.nextPage();
      }
    });

    root.addEventListener('submit', function (e) {
      var form = e.target.closest('[data-appointment-form]');
      if (!form) return;
      if (typeof e.preventDefault === 'function') e.preventDefault();

      var patientEl = form.querySelector('[name="patient"]');
      var providerEl = form.querySelector('[name="provider"]');
      var typeEl = form.querySelector('[name="appointment_type"]');
      var datetimeEl = form.querySelector('[name="scheduled_at"]');
      var durationEl = form.querySelector('[name="duration_minutes"]');
      var notesEl = form.querySelector('[name="notes"]');

      var newApt = {
        id: Date.now(),
        patient: patientEl ? patientEl.value.trim() : '',
        provider: providerEl ? providerEl.value.trim() : '',
        appointment_type: typeEl ? typeEl.value : 'Examination',
        scheduled_at: datetimeEl ? datetimeEl.value : new Date().toISOString(),
        duration_minutes: durationEl ? parseInt(durationEl.value, 10) || 30 : 30,
        notes: notesEl ? notesEl.value.trim() : '',
        status: 'Scheduled',
      };

      self.scheduleAppointment(newApt);
      self._showForm = false;
      self._render();
    });

    this._root = root;
  };

  AppointmentScheduling.prototype.getPageCount = function () {
    return Math.max(1, Math.ceil(this._all.length / this._cfg.pageSize));
  };

  AppointmentScheduling.prototype._render = function () {
    if (!this._root) return;

    var pageCount = this.getPageCount();
    if (this._page > pageCount) this._page = pageCount;
    if (this._page < 1) this._page = 1;

    var start = (this._page - 1) * this._cfg.pageSize;
    var rows = this._all.slice(start, start + this._cfg.pageSize);

    this._root.innerHTML = buildHTML({
      title: this._cfg.title,
      rows: rows,
      page: this._page,
      pageCount: pageCount,
      loading: this._loading,
      error: this._error,
      showForm: this._showForm,
      total: this._all.length,
    });
  };

  AppointmentScheduling.prototype.setAppointments = function (appointments) {
    this._all = parseArray(appointments);
    this._page = 1;
    this._loading = false;
    this._error = '';
    this._render();
  };

  AppointmentScheduling.prototype.scheduleAppointment = function (appointmentData) {
    if (!appointmentData) return;
    var apt = Object.assign(
      {
        id: Date.now(),
        status: 'Scheduled',
        duration_minutes: 30,
      },
      appointmentData
    );

    this._all.unshift(apt);
    if (typeof this._cfg.onSchedule === 'function') {
      try {
        this._cfg.onSchedule(apt);
      } catch (_) {}
    }
    this._page = 1;
    this._render();
    return apt;
  };

  AppointmentScheduling.prototype.confirmAppointment = function (id) {
    var match = this._all.find(function (a) {
      return String(a.id) === String(id);
    });
    if (match) {
      match.status = 'Confirmed';
      if (typeof this._cfg.onStatusChange === 'function') {
        try {
          this._cfg.onStatusChange(id, 'Confirmed');
        } catch (_) {}
      }
      this._render();
    }
  };

  AppointmentScheduling.prototype.cancelAppointment = function (id) {
    var match = this._all.find(function (a) {
      return String(a.id) === String(id);
    });
    if (match) {
      match.status = 'Cancelled';
      if (typeof this._cfg.onStatusChange === 'function') {
        try {
          this._cfg.onStatusChange(id, 'Cancelled');
        } catch (_) {}
      }
      this._render();
    }
  };

  AppointmentScheduling.prototype.rescheduleAppointment = function (id, newTime) {
    var match = this._all.find(function (a) {
      return String(a.id) === String(id);
    });
    if (match) {
      match.status = 'Rescheduled';
      match.scheduled_at = newTime;
      if (typeof this._cfg.onStatusChange === 'function') {
        try {
          this._cfg.onStatusChange(id, 'Rescheduled');
        } catch (_) {}
      }
      this._render();
    }
  };

  AppointmentScheduling.prototype.load = function () {
    var self = this;
    var provider = this._cfg.fetchAppointments;
    if (typeof provider !== 'function') return Promise.resolve(this._all);

    this.showLoading();

    return Promise.resolve()
      .then(function () {
        return provider();
      })
      .then(function (rows) {
        self.setAppointments(rows);
        return self._all;
      })
      .catch(function (err) {
        var msg = err && err.message ? err.message : 'Could not load appointments.';
        self.showError(msg);
        return [];
      });
  };

  AppointmentScheduling.prototype.showLoading = function () {
    this._loading = true;
    this._error = '';
    this._render();
  };

  AppointmentScheduling.prototype.showError = function (msg) {
    this._loading = false;
    this._error = msg || 'An error occurred';
    this._render();
  };

  AppointmentScheduling.prototype.goToPage = function (page) {
    var next = Math.min(Math.max(1, parseInt(page, 10) || 1), this.getPageCount());
    if (next === this._page) return;
    this._page = next;
    this._render();
  };

  AppointmentScheduling.prototype.nextPage = function () {
    this.goToPage(this._page + 1);
  };

  AppointmentScheduling.prototype.prevPage = function () {
    this.goToPage(this._page - 1);
  };

  AppointmentScheduling.prototype.destroy = function () {
    if (this._root && this._root.parentNode) {
      this._root.parentNode.removeChild(this._root);
    }
    this._root = null;
    this._mount = null;
    this._all = [];
    this._cfg = null;
  };

  return AppointmentScheduling;
});
