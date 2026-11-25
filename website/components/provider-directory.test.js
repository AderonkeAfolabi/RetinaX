/* ==========================================================================
   Functional tests for provider-directory.js (Issue #479)
   Runs with Node.js test runner: node --test website/components/provider-directory.test.js
   ========================================================================== */

const test = require('node:test');
const assert = require('node:assert/strict');

/* --------------------------------------------------------------------------
   Minimal DOM shim
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

const ProviderDirectory = require('./provider-directory.js');

/* --------------------------------------------------------------------------
   Fixtures & Helpers
   -------------------------------------------------------------------------- */

function makeProvider(overrides) {
  return Object.assign(
    {
      id: 'prov-001',
      name: 'Dr. Sarah Chen, MD',
      address: 'GDMOFXN476P775T546T6F6N4M6776P775T546T6F6N4M6776P775T546',
      specialty: 'Ophthalmology',
      status: 'Verified',
      verified: true,
      clinic: 'Metropolitan Eye Center',
      location: 'Boston, MA',
      rating: 4.9,
      languages: ['English', 'Mandarin'],
    },
    overrides || {}
  );
}

function mountDirectory(opts, dataset) {
  const mount = new MockEl(dataset);
  const directory = new ProviderDirectory(mount, opts);
  const root = mount.children[0];
  return { mount, directory, root };
}

/* --------------------------------------------------------------------------
   Tests
   -------------------------------------------------------------------------- */

test('constructor rejects non-HTMLElement mount', () => {
  assert.throws(() => new ProviderDirectory(null, {}), TypeError);
  assert.throws(() => new ProviderDirectory({}, {}), TypeError);
});

test('renders provider directory with expected cards and details', () => {
  const { root } = mountDirectory({ providers: [makeProvider()] });
  const html = root.innerHTML;

  assert.ok(html.includes('Provider Directory'), 'should render title');
  assert.ok(html.includes('Dr. Sarah Chen, MD'), 'should render provider name');
  assert.ok(html.includes('Ophthalmology'), 'should render specialty');
  assert.ok(html.includes('Verified'), 'should render verified status');
  assert.ok(html.includes('Metropolitan Eye Center'), 'should render clinic');
  assert.ok(html.includes('Boston, MA'), 'should render location');
  assert.ok(html.includes('4.9'), 'should render rating');
  assert.ok(html.includes('English, Mandarin'), 'should render languages');
});

test('filters providers by search query across name, clinic, location', () => {
  const providers = [
    makeProvider({
      id: '1',
      name: 'Dr. Sarah Chen',
      clinic: 'Boston Clinic',
      location: 'Boston, MA',
    }),
    makeProvider({
      id: '2',
      name: 'Dr. Marcus Vance',
      clinic: 'Midwest Vision',
      location: 'Chicago, IL',
    }),
    makeProvider({
      id: '3',
      name: 'Dr. Elena Rostova',
      clinic: 'Pacific Eye',
      location: 'Seattle, WA',
    }),
  ];

  const { directory, root } = mountDirectory({ providers });

  // Search by name
  directory.setSearchQuery('vance');
  assert.ok(root.innerHTML.includes('Dr. Marcus Vance'));
  assert.ok(!root.innerHTML.includes('Dr. Sarah Chen'));

  // Search by clinic
  directory.setSearchQuery('pacific');
  assert.ok(root.innerHTML.includes('Dr. Elena Rostova'));
  assert.ok(!root.innerHTML.includes('Dr. Marcus Vance'));

  // Search by location
  directory.setSearchQuery('chicago');
  assert.ok(root.innerHTML.includes('Dr. Marcus Vance'));

  // Clear search
  directory.setSearchQuery('');
  assert.ok(root.innerHTML.includes('Dr. Sarah Chen'));
  assert.ok(root.innerHTML.includes('Dr. Marcus Vance'));
  assert.ok(root.innerHTML.includes('Dr. Elena Rostova'));
});

test('filters providers by specialty', () => {
  const providers = [
    makeProvider({ id: '1', name: 'Dr. Alice', specialty: 'Optometry' }),
    makeProvider({ id: '2', name: 'Dr. Bob', specialty: 'Retina Specialist' }),
    makeProvider({ id: '3', name: 'Dr. Charlie', specialty: 'Optometry' }),
  ];

  const { directory, root } = mountDirectory({ providers });

  directory.setSpecialtyFilter('Optometry');
  assert.ok(root.innerHTML.includes('Dr. Alice'));
  assert.ok(root.innerHTML.includes('Dr. Charlie'));
  assert.ok(!root.innerHTML.includes('Dr. Bob'));

  directory.setSpecialtyFilter('Retina Specialist');
  assert.ok(root.innerHTML.includes('Dr. Bob'));
  assert.ok(!root.innerHTML.includes('Dr. Alice'));

  directory.setSpecialtyFilter('All');
  assert.ok(root.innerHTML.includes('Dr. Alice'));
  assert.ok(root.innerHTML.includes('Dr. Bob'));
});

test('filters providers by verification status', () => {
  const providers = [
    makeProvider({ id: '1', name: 'Dr. Verified', status: 'Verified', verified: true }),
    makeProvider({ id: '2', name: 'Dr. Pending', status: 'Pending', verified: false }),
  ];

  const { directory, root } = mountDirectory({ providers });

  directory.setStatusFilter('Verified');
  assert.ok(root.innerHTML.includes('Dr. Verified'));
  assert.ok(!root.innerHTML.includes('Dr. Pending'));

  directory.setStatusFilter('Pending');
  assert.ok(root.innerHTML.includes('Dr. Pending'));
  assert.ok(!root.innerHTML.includes('Dr. Verified'));
});

test('paginates providers and respects page boundaries', () => {
  const providers = Array.from({ length: 15 }, (_, i) =>
    makeProvider({ id: `p-${i + 1}`, name: `Provider #${i + 1}` })
  );

  const { directory, root } = mountDirectory({ providers, pageSize: 5 });

  assert.ok(root.innerHTML.includes('Page 1 of 3'));
  assert.ok(root.innerHTML.includes('Provider #1'));

  directory.nextPage();
  assert.ok(root.innerHTML.includes('Page 2 of 3'));
  assert.ok(root.innerHTML.includes('Provider #6'));

  directory.nextPage();
  assert.ok(root.innerHTML.includes('Page 3 of 3'));
  assert.ok(root.innerHTML.includes('Provider #15'));

  directory.nextPage();
  assert.ok(root.innerHTML.includes('Page 3 of 3'), 'should clamp at last page');

  directory.prevPage();
  assert.ok(root.innerHTML.includes('Page 2 of 3'));

  directory.goToPage(1);
  assert.ok(root.innerHTML.includes('Page 1 of 3'));
});

test('displays empty state when no providers match filters', () => {
  const { directory, root } = mountDirectory({ providers: [makeProvider()] });
  directory.setSearchQuery('nonexistent_doctor_123');

  assert.ok(root.innerHTML.includes('No providers match your search criteria.'));
});

test('shows loading and error states via load() and showError()', async () => {
  const { directory, root } = mountDirectory({
    fetchProviders: async () => {
      throw new Error('Directory service unreachable');
    },
  });

  await directory.load();
  assert.ok(root.innerHTML.includes('role="alert"'));
  assert.ok(root.innerHTML.includes('Directory service unreachable'));
});

test('escapes HTML from provider fields to prevent XSS', () => {
  const { root } = mountDirectory({
    providers: [
      makeProvider({
        name: '<script>alert("xss")</script>',
        clinic: '<b onmouseover=alert(1)>Eye Clinic</b>',
      }),
    ],
  });

  assert.ok(!root.innerHTML.includes('<script>'));
  assert.ok(root.innerHTML.includes('&lt;script&gt;'));
  assert.ok(!root.innerHTML.includes('<b onmouseover'));
  assert.ok(root.innerHTML.includes('&lt;b onmouseover'));
});

test('destroy cleans up DOM and references', () => {
  const { mount, directory } = mountDirectory({ providers: [makeProvider()] });
  assert.equal(mount.children.length, 1);

  directory.destroy();
  assert.equal(mount.children.length, 0);
  assert.equal(directory._root, null);
  assert.equal(directory._mount, null);
});
