import init, { verifyProofOfReserves } from './bdk_reserves_web.js'

const form = document.getElementById('form')
const submit = document.getElementById('submit')
const example = document.getElementById('example')
const result = document.getElementById('result')

const SATS_PER_BTC = 100_000_000n

// Set when the form holds the bundled example, so a failure can say that it is
// expected. Typing in any field clears it; assigning .value does not fire input.
let loadedExample = null
form.addEventListener('input', () => {
  loadedExample = null
})

function formatBtc(sats) {
  const value = BigInt(sats)
  const whole = value / SATS_PER_BTC
  const fraction = (value % SATS_PER_BTC).toString().padStart(8, '0')
  return `${whole.toLocaleString('en-US')}.${fraction}`
}

function show(kind, html) {
  result.className = kind
  result.innerHTML = html
}

function escape(text) {
  const node = document.createElement('div')
  node.textContent = text
  return node.innerHTML
}

// bdk-reserves reports its verdict as a Rust enum variant, which is precise but
// not much help to someone holding a proof that just failed. Say what each one
// actually means. The raw message is still shown underneath.
function explain(message) {
  const nonSpendable = message.match(/NonSpendableInput\((\d+)\)/)
  if (nonSpendable) {
    return `The proof claims input ${nonSpendable[1]} (counting the challenge input as 0), but the
            Esplora server does not list that output as unspent. Either those coins have been spent
            since the proof was made, or they are not buried deep enough for the confirmation
            setting above.`
  }
  if (message.includes('ChallengeInputMismatch')) {
    return 'The proof was made for a different message. It has to match exactly, including punctuation and capitalisation.'
  }
  const notSigned = message.match(/NotSignedInput\((\d+)\)/)
  if (notSigned) {
    return `Input ${notSigned[1]} carries no signature, so the proof does not demonstrate control of those coins.`
  }
  if (message.includes('SignatureValidation')) {
    return 'A signature in the proof is not valid for the output it spends.'
  }
  if (message.includes('InAndOutValueNotEqual')) {
    return 'The amounts going in and out of the proof do not match, so it is not a well formed proof of reserves.'
  }
  if (message.includes('UnsupportedSighashType')) {
    return 'An input is signed with a sighash type other than SIGHASH_ALL, which a proof of reserves cannot use.'
  }
  if (message.includes('WrongNumberOfInputs') || message.includes('WrongNumberOfOutputs')) {
    return 'The PSBT is not shaped like a BIP-127 proof: it should have one challenge input, one input per UTXO, and a single unspendable output.'
  }
  if (message.includes('InvalidOutput')) {
    return 'The proof does not pay to the unspendable output BIP-127 requires.'
  }
  return null
}

// Addresses are accepted one per line or comma separated, whichever is handier
// to paste.
function parseAddresses(raw) {
  return raw
    .split(/[\s,]+/)
    .map((address) => address.trim())
    .filter(Boolean)
}

// The example proof is fetched rather than inlined, because the PSBT alone is
// 28kB of base64 and most visitors never ask for it.
example.addEventListener('click', async () => {
  example.disabled = true
  try {
    const meta = await fetch('example/proof.json').then((response) => response.json())
    const psbt = await fetch(`example/${meta.proof_psbt}`).then((response) => response.text())

    document.getElementById('addresses').value = meta.addresses.join('\n')
    document.getElementById('message').value = meta.message
    document.getElementById('psbt').value = psbt.trim()
    document.getElementById('confirmations').value = meta.confirmations

    loadedExample = meta
    show('info', `<h2>Example loaded</h2><p class="meta">${escape(meta.note)}</p>`)
  } catch (error) {
    show('err', `<h2>Could not load the example</h2><p class="err-body">${escape(String(error))}</p>`)
  } finally {
    example.disabled = false
  }
})

form.addEventListener('submit', async (event) => {
  event.preventDefault()
  submit.disabled = true
  submit.textContent = 'Verifying…'
  show('', '')

  const request = {
    addresses: parseAddresses(document.getElementById('addresses').value),
    message: document.getElementById('message').value,
    proof_psbt: document.getElementById('psbt').value,
    // An emptied field means "the default", not zero confirmations.
    confirmations: document.getElementById('confirmations').value === ''
      ? null
      : Number(document.getElementById('confirmations').value),
    esplora_url: document.getElementById('esplora').value.trim() || null,
  }

  try {
    const proof = await verifyProofOfReserves(request)
    show(
      'ok',
      `<h2>Proof verified</h2>
       <div class="amount">${formatBtc(proof.spendable)} BTC</div>
       <p class="meta">
         ${proof.spendable.toLocaleString('en-US')} sats across ${proof.utxos}
         UTXO${proof.utxos === 1 ? '' : 's'} on ${escape(proof.network)},
         confirmed as of block ${proof.tip_height.toLocaleString('en-US')}.<br>
         Looked up via ${escape(proof.esplora_url)}
       </p>`,
    )
  } catch (error) {
    const message = String(error)
    const reason = explain(message)
    const expected =
      loadedExample && message.includes('NonSpendableInput')
        ? `<p class="meta">This is the expected result for the bundled example. ${escape(loadedExample.note)}</p>`
        : ''
    show(
      'err',
      `<h2>Not verified</h2>
       ${reason ? `<p>${reason}</p>` : ''}
       ${expected}
       <p class="err-body">${escape(message)}</p>`,
    )
  } finally {
    submit.disabled = false
    submit.textContent = 'Verify'
  }
})

init()
  .then(() => {
    submit.disabled = false
    submit.textContent = 'Verify'
  })
  .catch((error) => {
    show('err', `<h2>Could not start</h2><p class="err-body">${escape(String(error))}</p>`)
    submit.textContent = 'Unavailable'
  })
