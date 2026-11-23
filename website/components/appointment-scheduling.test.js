/* ==========================================================================
   Functional tests for appointment-scheduling.js (Issue #477)
   Runs with Node.js test runner: node --test website/components/appointment-scheduling.test.js
   ========================================================================== */

const test = require('node:test');
const assert = require('node:assert/strict');

/* --------------------------------------------------------------------------
   Minimal DOM shim (matches prescription-list.test.js)
   -------------------------------------------------------------------------- */

global.HTMLElement = function HTMLElement() {};

function MockEl(dataset) {
  const el = this;
  el.dataset = dataset || {};
  el.tagName = 'DIV';
  el.className = '';
  el.innerHTML = '';
  el.children = [];
  el.parentNode = null;
  el._attributes = {};
  el._listeners = {};

  el.classList = {
    add(name) {
      if (!el.classList.contains(name)) {
        el.className = el.className ? `${el.className} ${name}` : name;
      }
    },
    remove(name) {
      el.className = el.className
        .split(/\s+/)
        .filter((cls) => cls && cls !== name)
        .join(' ');
    },
    contains(name) {
      return el.className.split(/\s+/).includes(name);
    },
  };
}

MockEl.prototype = Object.create(global.HTMLElement.prototype);
MockEl.prototype.constructor = MockEl;

MockEl.prototype.appendChild = function (child) {
  this.children.push(child);
  child.parentNode = this;
  return child;
};

MockEl.prototype.removeChild = function (child) {
  const index = this.children.indexOf(child);
  if (index !== -1) this.children.splice(index, 1);
  child.parentNode = null;
  return child;
};

MockEl.prototype.querySelector = function () {
  return null;
};

MockEl.prototype.querySelectorAll = function () {
  return [];
};

MockEl.prototype.setAttribute = function (name, value) {
  this._attributes[name] = String(value);
};

MockEl.prototype.getAttribute = function (name) {
  return Object.prototype.hasOwnProperty.call(this._attributes, name)
    ? this._attributes[name]
    : null;
};

MockEl.prototype.addEventListener = function (type, handler) {
  if (!this._listeners[type]) this._listeners[type] = [];
  this._listeners[type].push(handler);
};

function createElement(tag) {
  const el = new MockEl();
  el.tagName = String(tag).toUpperCase();
  return el;
}

global.document = {
  readyState: 'complete',
  querySelectorAll: () => [],
  addEventListener: () => {},
  createElement,
};
global.window = global;

const AppointmentScheduling = require('./appointment-scheduling.js');

/* --------------------------------------------------------------------------
   Fixtures & Helpers
   -------------------------------------------------------------------------- */

function makeApt(overrides) {
  return Object.assign(
    {
      id: 101,
      patient: 'GDMOFXN476P775T546T6F6N4M6776P775T546T6F6N4M6776P775T546',
      provider: 'GBNDFXN476P775T546T6F6N4M6776P775T546T6F6N4M6776P775T546',
      appointment_type: 'Examination',
      scheduled_at: 1716300000,
      duration_minutes: 30,
      status: 'Scheduled',
      notes: 'Routine refraction scan',
    },
    overrides || {}
  );
}

function mountScheduler(opts, dataset) {
  const mount = new MockEl(dataset);
  const scheduler = new AppointmentScheduling(mount, opts);
  const root = mount.children[0];
  return { mount, scheduler, root };
}

/* --------------------------------------------------------------------------
   Tests
   -------------------------------------------------------------------------- */

test('constructor rejects non-HTMLElement mount', () => {
  assert.throws(() => new AppointmentScheduling(null, {}), TypeError);
  assert.throws(() => new AppointmentScheduling({}, {}), TypeError);
});

test('renders appointments table with expected columns and data', () => {
  const { root } = mountScheduler({ appointments: [makeApt()] });
  const html = root.innerHTML;

  assert.ok(html.includes('Appointment Scheduling'), 'should render title');
  assert.ok(html.includes('#101'), 'should render ID');
  assert.ok(html.includes('Examination'), 'should render type');
  assert.ok(html.includes('Scheduled'), 'should render status');
  assert.ok(html.includes('30 min'), 'should render duration');
  assert.ok(html.includes('Confirm'), 'should show action button');
  assert.ok(html.includes('Cancel'), 'should show cancel button');
});

test('supports all clinical appointment types and status states', () => {
  const apts = [
    makeApt({ id: 1, appointment_type: 'Consultation', status: 'Confirmed' }),
    makeApt({ id: 2, appointment_type: 'Surgery', status: 'Completed' }),
    makeApt({ id: 3, appointment_type: 'Emergency', status: 'Cancelled' }),
    makeApt({ id: 4, appointment_type: 'FollowUp', status: 'Rescheduled' }),
  ];

  const { root } = mountScheduler({ appointments: apts, pageSize: 10 });
  const html = root.innerHTML;

  assert.ok(html.includes('Consultation'), 'should render consultation type');
  assert.ok(html.includes('Surgery'), 'should render surgery type');
  assert.ok(html.includes('Emergency'), 'should render emergency type');
  assert.ok(html.includes('Follow-Up'), 'should render followup type');

  assert.ok(html.includes('Confirmed'), 'should render confirmed status');
  assert.ok(html.includes('Completed'), 'should render completed status');
  assert.ok(html.includes('Cancelled'), 'should render cancelled status');
  assert.ok(html.includes('Rescheduled'), 'should render rescheduled status');
});

test('paginates appointments and navigates correctly', () => {
  const apts = [
    makeApt({ id: 1 }),
    makeApt({ id: 2 }),
    makeApt({ id: 3 }),
    makeApt({ id: 4 }),
    makeApt({ id: 5 }),
    makeApt({ id: 6 }),
  ];

  const { scheduler, root } = mountScheduler({ appointments: apts, pageSize: 2 });

  assert.ok(root.innerHTML.includes('Page 1 of 3'), 'should start on page 1');
  assert.ok(root.innerHTML.includes('#1'), 'should show apt 1');

  scheduler.nextPage();
  assert.ok(root.innerHTML.includes('Page 2 of 3'), 'should advance to page 2');
  assert.ok(root.innerHTML.includes('#3'), 'should show apt 3');

  scheduler.nextPage();
  assert.ok(root.innerHTML.includes('Page 3 of 3'), 'should advance to page 3');

  scheduler.nextPage();
  assert.ok(root.innerHTML.includes('Page 3 of 3'), 'should clamp at page 3');

  scheduler.prevPage();
  assert.ok(root.innerHTML.includes('Page 2 of 3'), 'should return to page 2');

  scheduler.goToPage(1);
  assert.ok(root.innerHTML.includes('Page 1 of 3'), 'should jump to page 1');
});

test('scheduleAppointment prepends new appointment and updates view', () => {
  const { scheduler, root } = mountScheduler({ appointments: [makeApt({ id: 1 })] });

  scheduler.scheduleAppointment({
    id: 999,
    patient: 'GPATIENTNEW',
    provider: 'GPROVIDERNEW',
    appointment_type: 'Surgery',
    status: 'Scheduled',
  });

  assert.ok(root.innerHTML.includes('#999'), 'should show new appointment');
  assert.ok(root.innerHTML.includes('Surgery'), 'should render new appointment type');
});

test('confirmAppointment updates status to Confirmed', () => {
  let statusChangedTo = null;
  const { scheduler, root } = mountScheduler({
    appointments: [makeApt({ id: 50, status: 'Scheduled' })],
    onStatusChange: (id, status) => {
      statusChangedTo = status;
    },
  });

  assert.ok(root.innerHTML.includes('appointment__status--scheduled'));

  scheduler.confirmAppointment(50);
  assert.ok(root.innerHTML.includes('appointment__status--confirmed'));
  assert.equal(statusChangedTo, 'Confirmed');
});

test('cancelAppointment updates status to Cancelled', () => {
  let statusChangedTo = null;
  const { scheduler, root } = mountScheduler({
    appointments: [makeApt({ id: 60, status: 'Scheduled' })],
    onStatusChange: (id, status) => {
      statusChangedTo = status;
    },
  });

  scheduler.cancelAppointment(60);
  assert.ok(root.innerHTML.includes('appointment__status--cancelled'));
  assert.equal(statusChangedTo, 'Cancelled');
});

test('shows loading and error states via load() and showError()', async () => {
  const { scheduler, root } = mountScheduler({
    fetchAppointments: async () => {
      throw new Error('Appointment service offline');
    },
  });

  await scheduler.load();
  assert.ok(root.innerHTML.includes('role="alert"'), 'should render alert');
  assert.ok(root.innerHTML.includes('Appointment service offline'), 'should render error message');
});

test('renders empty state when no appointments are present', () => {
  const { root } = mountScheduler({ appointments: [] });
  assert.ok(root.innerHTML.includes('No appointments found.'), 'should show empty state');
});

test('escapes HTML to prevent XSS injection', () => {
  const { root } = mountScheduler({
    appointments: [
      makeApt({
        notes: '<script>alert("xss")</script>',
        patient: '<img src=x onerror=alert(1)>',
      }),
    ],
  });

  assert.ok(!root.innerHTML.includes('<script>'), 'must not emit script tag');
  assert.ok(!root.innerHTML.includes('<img src=x'), 'must not emit image tag');
});

test('destroy cleans up DOM and references', () => {
  const { mount, scheduler } = mountScheduler({ appointments: [makeApt()] });
  assert.equal(mount.children.length, 1);

  scheduler.destroy();
  assert.equal(mount.children.length, 0);
  assert.equal(scheduler._root, null);
  assert.equal(scheduler._mount, null);
});
