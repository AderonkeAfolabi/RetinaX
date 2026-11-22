/* ==========================================================================
   Tests for Accordion Component and Focus Trap Navigation (Issue #475)
   Runs with Node.js test runner: node --test website/components/accordion.test.js
   ========================================================================== */

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  getNextAccordionIndex,
  getFocusableElements,
  trapFocus,
  createFocusTrap,
  initAccordion,
} = require('./accordion.js');

test('getNextAccordionIndex navigates sequentially and wraps boundaries', () => {
  const count = 4;
  // ArrowDown
  assert.equal(getNextAccordionIndex(0, count, 'ArrowDown'), 1);
  assert.equal(getNextAccordionIndex(1, count, 'ArrowDown'), 2);
  assert.equal(getNextAccordionIndex(3, count, 'ArrowDown'), 0); // wrap

  // ArrowRight (behaves like ArrowDown)
  assert.equal(getNextAccordionIndex(3, count, 'ArrowRight'), 0);

  // ArrowUp
  assert.equal(getNextAccordionIndex(2, count, 'ArrowUp'), 1);
  assert.equal(getNextAccordionIndex(0, count, 'ArrowUp'), 3); // wrap

  // ArrowLeft (behaves like ArrowUp)
  assert.equal(getNextAccordionIndex(0, count, 'ArrowLeft'), 3);

  // Home & End
  assert.equal(getNextAccordionIndex(2, count, 'Home'), 0);
  assert.equal(getNextAccordionIndex(1, count, 'End'), 3);

  // Unhandled key keeps position
  assert.equal(getNextAccordionIndex(2, count, 'Enter'), 2);
});

test('getFocusableElements finds interactive elements and ignores disabled', () => {
  const container = {
    querySelectorAll: (selector) => {
      return [
        { tagName: 'BUTTON', disabled: false, tabIndex: 0 },
        { tagName: 'A', href: '#', tabIndex: 0 },
        { tagName: 'INPUT', disabled: false, tabIndex: 0 },
      ];
    },
  };

  const focusable = getFocusableElements(container);
  assert.equal(focusable.length, 3);
});

test('trapFocus wraps focus from last to first on Tab', () => {
  let focused = null;
  const btn1 = {
    tabIndex: 0,
    focus: () => {
      focused = btn1;
    },
  };
  const btn2 = {
    tabIndex: 0,
    focus: () => {
      focused = btn2;
    },
  };

  const container = {
    querySelectorAll: () => [btn1, btn2],
    contains: (el) => el === btn1 || el === btn2,
  };

  // Mock document.activeElement as last element (btn2)
  global.document = { activeElement: btn2 };

  let prevented = false;
  const event = {
    key: 'Tab',
    shiftKey: false,
    preventDefault: () => {
      prevented = true;
    },
  };

  const handled = trapFocus(container, event);
  assert.equal(handled, true);
  assert.equal(prevented, true);
  assert.equal(focused, btn1, 'should wrap focus to first element');
});

test('trapFocus wraps focus from first to last on Shift+Tab', () => {
  let focused = null;
  const btn1 = {
    tabIndex: 0,
    focus: () => {
      focused = btn1;
    },
  };
  const btn2 = {
    tabIndex: 0,
    focus: () => {
      focused = btn2;
    },
  };

  const container = {
    querySelectorAll: () => [btn1, btn2],
    contains: (el) => el === btn1 || el === btn2,
  };

  // Mock document.activeElement as first element (btn1)
  global.document = { activeElement: btn1 };

  let prevented = false;
  const event = {
    key: 'Tab',
    shiftKey: true,
    preventDefault: () => {
      prevented = true;
    },
  };

  const handled = trapFocus(container, event);
  assert.equal(handled, true);
  assert.equal(prevented, true);
  assert.equal(focused, btn2, 'should wrap focus to last element');
});

test('createFocusTrap activates, handles Escape, and restores focus on release', () => {
  let restored = false;
  let closed = false;
  const previousButton = {
    focus: () => {
      restored = true;
    },
  };

  let listeners = {};
  global.document = {
    activeElement: previousButton,
    addEventListener: (type, fn) => {
      listeners[type] = fn;
    },
    removeEventListener: (type, fn) => {
      delete listeners[type];
    },
  };

  const modalEl = {
    querySelectorAll: () => [],
    focus: () => {},
  };

  const trap = createFocusTrap(modalEl, {
    onEscape: () => {
      closed = true;
    },
    restoreFocus: true,
  });

  // Trigger Escape
  listeners['keydown']({ key: 'Escape', preventDefault: () => {} });
  assert.equal(closed, true, 'should invoke onEscape handler');

  // Release trap
  trap.release();
  assert.equal(restored, true, 'should restore focus to previous active element');
});

test('initAccordion binds click toggle and arrow key navigation', () => {
  const listeners = {};
  let focusedEl = null;

  function makeTrigger(index) {
    const trigger = {
      index,
      attributes: { 'aria-expanded': 'false' },
      getAttribute(name) {
        return this.attributes[name] || null;
      },
      setAttribute(name, val) {
        this.attributes[name] = String(val);
      },
      addEventListener(event, fn) {
        if (!listeners[index]) listeners[index] = {};
        listeners[index][event] = fn;
      },
      focus() {
        focusedEl = this;
      },
    };
    return trigger;
  }

  const triggers = [makeTrigger(0), makeTrigger(1), makeTrigger(2)];
  const container = {
    querySelectorAll: (sel) => (sel === '.accordion__trigger' ? triggers : []),
  };

  initAccordion(container);

  // Test click toggle
  listeners[0]['click']();
  assert.equal(triggers[0].getAttribute('aria-expanded'), 'true');
  listeners[0]['click']();
  assert.equal(triggers[0].getAttribute('aria-expanded'), 'false');

  // Test arrow down navigation
  listeners[0]['keydown']({ key: 'ArrowDown', preventDefault: () => {} });
  assert.equal(focusedEl, triggers[1], 'ArrowDown should focus trigger 1');

  // Test wrap from last to first
  listeners[2]['keydown']({ key: 'ArrowDown', preventDefault: () => {} });
  assert.equal(focusedEl, triggers[0], 'ArrowDown on last should wrap to trigger 0');

  // Test wrap backwards from first to last
  listeners[0]['keydown']({ key: 'ArrowUp', preventDefault: () => {} });
  assert.equal(focusedEl, triggers[2], 'ArrowUp on first should wrap to trigger 2');

  // Test End key
  listeners[0]['keydown']({ key: 'End', preventDefault: () => {} });
  assert.equal(focusedEl, triggers[2], 'End key should focus last trigger');

  // Test Home key
  listeners[2]['keydown']({ key: 'Home', preventDefault: () => {} });
  assert.equal(focusedEl, triggers[0], 'Home key should focus first trigger');
});
