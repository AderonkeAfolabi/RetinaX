/* ==========================================================================
   RetinaX — Accordion Component
   Accessible accordion with smooth CSS grid height transitions.
   Follows WAI-ARIA Accordion Pattern & Keyboard Navigation.
   Also provides modal focus trap & restore utilities.
   ========================================================================== */
(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.Accordion = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  /**
   * Determine target index for keyboard navigation in an accordion.
   * WAI-ARIA pattern:
   *   ArrowDown / ArrowRight: next header (wraps)
   *   ArrowUp / ArrowLeft: previous header (wraps)
   *   Home: first header
   *   End: last header
   */
  function getNextAccordionIndex(currentIndex, count, key) {
    if (!count || count <= 0) return 0;
    var idx = typeof currentIndex === 'number' ? currentIndex : 0;
    switch (key) {
      case 'ArrowDown':
      case 'ArrowRight':
        return (idx + 1) % count;
      case 'ArrowUp':
      case 'ArrowLeft':
        return (idx - 1 + count) % count;
      case 'Home':
        return 0;
      case 'End':
        return count - 1;
      default:
        return idx;
    }
  }

  /**
   * Query all interactive focusable elements within a container.
   */
  var FOCUSABLE_SELECTORS = [
    'a[href]',
    'area[href]',
    'input:not([disabled]):not([type="hidden"])',
    'select:not([disabled])',
    'textarea:not([disabled])',
    'button:not([disabled])',
    'iframe',
    'object',
    'embed',
    '[contenteditable]',
    '[tabindex]:not([tabindex="-1"])',
  ].join(',');

  function getFocusableElements(container) {
    if (!container || typeof container.querySelectorAll !== 'function') return [];
    var elements = Array.prototype.slice.call(container.querySelectorAll(FOCUSABLE_SELECTORS));
    return elements.filter(function (el) {
      return (
        el.offsetWidth > 0 ||
        el.offsetHeight > 0 ||
        (typeof el.getClientRects === 'function' && el.getClientRects().length > 0) ||
        el.tabIndex >= 0
      );
    });
  }

  /**
   * Trap Tab focus within a modal container.
   * Returns true if event was handled and default was prevented.
   */
  function trapFocus(container, event) {
    if (!container || !event || event.key !== 'Tab') return false;
    var focusable = getFocusableElements(container);
    if (focusable.length === 0) {
      if (typeof event.preventDefault === 'function') event.preventDefault();
      return true;
    }

    var first = focusable[0];
    var last = focusable[focusable.length - 1];
    var active =
      typeof document !== 'undefined' && document.activeElement ? document.activeElement : null;

    if (event.shiftKey) {
      if (!active || active === first || !container.contains(active)) {
        if (typeof event.preventDefault === 'function') event.preventDefault();
        if (typeof last.focus === 'function') last.focus();
        return true;
      }
    } else {
      if (!active || active === last || !container.contains(active)) {
        if (typeof event.preventDefault === 'function') event.preventDefault();
        if (typeof first.focus === 'function') first.focus();
        return true;
      }
    }
    return false;
  }

  /**
   * Create an accessible focus trap session for a modal dialog.
   */
  function createFocusTrap(modalEl, options) {
    var opts = options || {};
    var previousActive =
      typeof document !== 'undefined' && document.activeElement ? document.activeElement : null;
    var active = true;

    function handleKeydown(e) {
      if (!active) return;
      if (e.key === 'Tab') {
        trapFocus(modalEl, e);
      } else if (e.key === 'Escape' && typeof opts.onEscape === 'function') {
        if (typeof e.preventDefault === 'function') e.preventDefault();
        opts.onEscape();
      }
    }

    if (typeof document !== 'undefined' && typeof document.addEventListener === 'function') {
      document.addEventListener('keydown', handleKeydown);
    }

    // Move initial focus
    var focusable = getFocusableElements(modalEl);
    if (focusable.length > 0 && typeof focusable[0].focus === 'function') {
      focusable[0].focus();
    } else if (modalEl && typeof modalEl.focus === 'function') {
      modalEl.focus();
    }

    return {
      release: function () {
        active = false;
        if (typeof document !== 'undefined' && typeof document.removeEventListener === 'function') {
          document.removeEventListener('keydown', handleKeydown);
        }
        if (
          opts.restoreFocus !== false &&
          previousActive &&
          typeof previousActive.focus === 'function'
        ) {
          try {
            previousActive.focus();
          } catch (_) {}
        }
      },
    };
  }

  function initAccordion(container) {
    if (!container) return;

    var triggers = Array.prototype.slice.call(container.querySelectorAll('.accordion__trigger'));

    triggers.forEach(function (trigger, index) {
      trigger.addEventListener('click', function () {
        var expanded = trigger.getAttribute('aria-expanded') === 'true';
        trigger.setAttribute('aria-expanded', String(!expanded));
      });

      trigger.addEventListener('keydown', function (event) {
        var key = event.key;
        if (
          key === 'ArrowDown' ||
          key === 'ArrowUp' ||
          key === 'ArrowLeft' ||
          key === 'ArrowRight' ||
          key === 'Home' ||
          key === 'End'
        ) {
          if (typeof event.preventDefault === 'function') event.preventDefault();
          var nextIdx = getNextAccordionIndex(index, triggers.length, key);
          if (triggers[nextIdx] && typeof triggers[nextIdx].focus === 'function') {
            triggers[nextIdx].focus();
          }
        }
      });
    });
  }

  function initAll() {
    if (typeof document === 'undefined') return;
    var accordions = document.querySelectorAll('[data-accordion]');
    Array.prototype.forEach.call(accordions, initAccordion);
  }

  if (typeof document !== 'undefined') {
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', initAll);
    } else {
      initAll();
    }
  }

  return {
    initAccordion: initAccordion,
    initAll: initAll,
    getNextAccordionIndex: getNextAccordionIndex,
    getFocusableElements: getFocusableElements,
    trapFocus: trapFocus,
    createFocusTrap: createFocusTrap,
    FOCUSABLE_SELECTORS: FOCUSABLE_SELECTORS,
  };
});
