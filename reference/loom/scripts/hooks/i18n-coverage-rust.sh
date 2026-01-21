#!/usr/bin/env bash
set -euo pipefail

LOCALES_DIR="crates/loom-common-i18n/locales"
EN_FILE="$LOCALES_DIR/en/messages.po"

if [[ ! -f "$EN_FILE" ]]; then
  echo "❌ English locale file not found: $EN_FILE"
  exit 1
fi

EN_KEYS=$(grep '^msgid "' "$EN_FILE" | sed 's/msgid "//' | sed 's/"$//' | grep -v '^$' | sort)
EN_COUNT=$(echo "$EN_KEYS" | wc -l)

echo "📋 English has $EN_COUNT translation keys (loom-common-i18n)"
echo ""

MISSING_FOUND=0

for locale in ar bn el es et fr he hi id it ja ko nl pt ru sv zh-CN; do
  LOCALE_FILE="$LOCALES_DIR/$locale/messages.po"

  if [[ ! -f "$LOCALE_FILE" ]]; then
    echo "⚠️  Locale file not found: $LOCALE_FILE"
    continue
  fi

  LOCALE_KEYS=$(grep '^msgid "' "$LOCALE_FILE" | sed 's/msgid "//' | sed 's/"$//' | grep -v '^$' | sort)
  MISSING=$(comm -23 <(echo "$EN_KEYS") <(echo "$LOCALE_KEYS") || true)
  MISSING_COUNT=$(echo "$MISSING" | grep -c . || true)

  if [[ $MISSING_COUNT -gt 0 ]]; then
    echo "❌ $locale: missing $MISSING_COUNT translations:"
    echo "${MISSING//$'\n'/$'\n'   - }" | sed '1s/^/   - /'
    echo ""
    MISSING_FOUND=1
  else
    echo "✅ $locale: complete"
  fi
done

echo ""

if [[ $MISSING_FOUND -eq 1 ]]; then
  echo "❌ FAILED: Some locales are missing translations!"
  exit 1
else
  echo "✅ All locales have complete translation coverage."
fi
