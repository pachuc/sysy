#!/usr/bin/env bash
set -euo pipefail
# Run with sysy on PATH and a path that does not already exist.
file=${1:?Usage: author-checkout.sh PATH}
sysy --json new "$file" --title 'Online checkout' --description 'Order placement, payment authorization, and asynchronous fulfillment.'
sysy --json container add "$file" commerce --label 'Commerce platform'
sysy --json container add "$file" payments --label 'Payment boundary' --in commerce
sysy --json container add "$file" fulfillment --label 'Fulfillment platform'
sysy --json node add "$file" shopper --kind client --label 'Shopper browser'
sysy --json node add "$file" gateway --kind service --label 'API gateway' --in commerce
sysy --json node add "$file" catalog --kind service --label 'Product catalog' --in commerce
sysy --json node add "$file" cache --kind cache --label 'Catalog cache' --in commerce
sysy --json node add "$file" checkout --kind service --label 'Checkout API' --in commerce
sysy --json node add "$file" orders --kind database --label 'Orders database' --in commerce
sysy --json node add "$file" payment --kind service --label 'Payment service' --in payments
sysy --json node add "$file" ledger --kind database --label 'Payment ledger' --in payments
sysy --json node add "$file" processor --kind external --label 'Payment processor'
sysy --json node add "$file" events --kind queue --label 'Order events' --in fulfillment
sysy --json node add "$file" inventory --kind service --label 'Inventory service' --in fulfillment
sysy --json node add "$file" warehouse --kind database --label 'Stock database' --in fulfillment
sysy --json node add "$file" receipt --kind function --label 'Receipt generator' --in fulfillment
sysy --json node add "$file" archive --kind storage --label 'Receipt archive' --in fulfillment
sysy --json edge add "$file" browse --from shopper --to gateway --kind sync --label 'Browse products'
sysy --json edge add "$file" find-products --from gateway --to catalog --kind sync --label 'Find products'
sysy --json edge add "$file" cached-products --from catalog --to cache --kind data --label 'Read product cache'
sysy --json edge add "$file" place-order --from gateway --to checkout --kind sync --label 'Place order'
sysy --json edge add "$file" price-order --from checkout --to catalog --kind sync --label 'Check prices'
sysy --json edge add "$file" save-order --from checkout --to orders --kind data --label 'Persist order'
sysy --json edge add "$file" authorize --from checkout --to payment --kind sync --label 'Authorize payment'
sysy --json edge add "$file" charge --from payment --to processor --kind sync --label 'Charge token'
sysy --json edge add "$file" record-charge --from payment --to ledger --kind data --label 'Record charge'
sysy --json edge add "$file" publish-order --from checkout --to events --kind async --label 'Order placed'
sysy --json edge add "$file" reserve-stock --from events --to inventory --kind async --label 'Reserve stock'
sysy --json edge add "$file" update-stock --from inventory --to warehouse --kind data --label 'Update quantities'
sysy --json edge add "$file" create-receipt --from events --to receipt --kind async --label 'Generate receipt'
sysy --json edge add "$file" read-order --from receipt --to orders --kind data --label 'Read order details'
sysy --json edge add "$file" store-receipt --from receipt --to archive --kind data --label 'Store PDF'
sysy --json edge add "$file" payment-contract --from commerce --to processor --kind dependency --label 'Merchant account'
sysy --json note add "$file" idempotency --on payment --text 'Retry with the same order key.'
sysy --json note add "$file" delivery --on publish-order --text 'Consumers deduplicate order IDs.'
sysy --json note add "$file" scope --text 'Refunds and returns are outside this view.'
sysy --json validate "$file"
sysy show "$file"
sysy --json layout "$file"
sysy --json node set "$file" checkout --description 'Coordinates pricing, payment, and the order outbox.'
sysy --json validate "$file"
