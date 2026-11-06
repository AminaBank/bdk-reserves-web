// End-to-end test of the built wasm module, driven through its real JS API
// against a mock Esplora server.
//
// The fixture proof is genuine and so are its signatures, but the UTXOs it
// covers were spent years ago, so a live server reports nothing unspent.
// Serving the real transactions back as though they were still unspent
// exercises the whole success path, consensus signature checks on all 18 inputs
// included, without needing a wallet or a regtest node.
//
// Run it with `make test-e2e`, which builds the module first.

const http = require('http')
const fs = require('fs')
const path = require('path')

const FIXTURES = path.join(__dirname, 'fixtures')
const EXAMPLE = path.join(__dirname, '..', 'web', 'example')

// Read the same metadata the "Load example" button loads, so the example the
// site offers and the proof this test asserts on cannot drift apart.
const example = JSON.parse(fs.readFileSync(path.join(EXAMPLE, 'proof_ok.json'), 'utf8'))
const MESSAGE = example.message
const PSBT = fs.readFileSync(path.join(EXAMPLE, example.proof_psbt), 'utf8').trim()
const TIP = 5194000
const EXPECTED_SATS = 922739

const wasm = require(path.join(__dirname, '..', 'target', 'e2e', 'bdk_reserves_web.js'))

const transactions = {}
for (const txid of fs.readdirSync(path.join(FIXTURES, 'transactions'))) {
  transactions[txid] = fs.readFileSync(path.join(FIXTURES, 'transactions', txid), 'utf8').trim()
}

const tx_status = {}
for (const txid of fs.readdirSync(path.join(FIXTURES, 'tx_status'))) {
  tx_status[txid] = fs.readFileSync(path.join(FIXTURES, 'tx_status', txid), 'utf8').trim()
}

const outspend = {}
for (const txid of fs.readdirSync(path.join(FIXTURES, 'outspend'))) {
  const osp = {}
  for (const vout of fs.readdirSync(path.join(FIXTURES, 'outspend', txid))) {
    osp[vout] = fs.readFileSync(path.join(FIXTURES, 'outspend', txid, vout), 'utf8').trim()
  }
  outspend[txid] = osp
}

const blocks = {}
for (const blk of fs.readdirSync(path.join(FIXTURES, 'blocks'))) {
  blocks[blk] = fs.readFileSync(path.join(FIXTURES, 'blocks', blk), 'utf8').trim()
}

const served = { tip: 0, utxo: 0, tx: 0 }

const server = http.createServer((req, res) => {
  const send = (code, body) => {
    res.writeHead(code, { 'content-type': 'text/plain' })
    res.end(body)
  }

  if (req.url === '/blocks/tip/height') {
    served.tip++
    return send(200, String(TIP))
  }

  if ((match = req.url.match(/^\/tx\/([0-9a-f]{64})\/hex$/))) {
    served.tx++
    return transactions[match[1]] ? send(200, transactions[match[1]]) : send(404, 'Not found')
  }

  if ((match = req.url.match(/^\/tx\/([0-9a-f]{64})\/status$/))) {
    return tx_status[match[1]] ? send(200, tx_status[match[1]]) : send(404, 'Not found')
  }

  if ((match = req.url.match(/^\/tx\/([0-9a-f]{64})\/outspend\/([0-9])$/))) {
    return outspend[match[1]][match[2]] ? send(200, outspend[match[1]][match[2]]) : send(404, 'Not found')
  }

  if ((match = req.url.match(/^\/blocks\/([0-9]+)$/))) {
    return blocks[match[1]] ? send(200, blocks[match[1]]) : send(404, 'Not found')
  }

  send(404, 'Not found')
})

const verify = (overrides = {}) =>
  wasm.verifyProofOfReserves({
    message: MESSAGE,
    proof_psbt: PSBT,
    confirmations: 3,
    esplora_url: `http://127.0.0.1:${server.address().port}`,
    ...overrides,
  })

let failures = 0
const check = (name, ok, detail) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'}  ${name}${detail ? '  ->  ' + detail : ''}`)
  if (!ok) failures++
}

const rejects = async (name, overrides, expected) => {
  try {
    await verify(overrides)
    check(name, false, 'the proof was accepted')
  } catch (error) {
    check(name, String(error).includes(expected), String(error))
  }
}

server.listen(0, '127.0.0.1', async () => {
  try {
    const proof = await verify()
    check('a valid proof verifies', proof.spendable === EXPECTED_SATS, `${proof.spendable} sats`)
    check('every outpoint is counted', proof.utxos === 19, String(proof.utxos))
    check('transaction fetches are deduped', served.tx === 16, `${served.tx} requests for 19 utxos`)
    check('latest valid time is reported', proof.valid_until == '2026-10-15T22:48:20Z', `reports to be valid until ${proof.valid_until}`)

    await rejects('a proof is bound to its message', { message: 'another message' }, 'ChallengeInputMismatch')

    await rejects('confirmation depth is enforced', { confirmations: 220000 }, 'NonSpendableInput')

    await rejects('a malformed psbt is refused', { proof_psbt: 'garbage!!' }, 'Base64 decode error')

    // The amounts come from the transactions rather than the UTXO listing, so a
    // server that swaps one in must be caught rather than believed.
    const [first, second, third] = Object.keys(transactions)
    const original = transactions[second]
    transactions[second] = transactions[third]
    await rejects('a substituted transaction is caught', {}, 'txid is')
    transactions[second] = original

    const again = await verify()
    check('a repeat run is stable', again.spendable === proof.spendable, `${again.spendable} sats`)
  } catch (error) {
    console.log('unexpected failure:', error)
    failures++
  }

  server.close()
  console.log(failures === 0 ? '\nAll checks passed.' : `\n${failures} check(s) failed.`)
  process.exit(failures === 0 ? 0 : 1)
})
