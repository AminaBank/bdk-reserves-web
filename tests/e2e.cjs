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
const example = JSON.parse(fs.readFileSync(path.join(EXAMPLE, 'proof.json'), 'utf8'))
const ADDRESS = example.addresses[0]
const MESSAGE = example.message
const PSBT = fs.readFileSync(path.join(EXAMPLE, example.proof_psbt), 'utf8').trim()
const TIP = 2000000
const EXPECTED_SATS = 53865580
const DEPTH_BELOW_TIP = 100
// The including block is the first confirmation, hence the +1.
const DEPTH = DEPTH_BELOW_TIP + 1

const wasm = require(path.join(__dirname, '..', 'target', 'e2e', 'bdk_reserves_web.js'))

const outpoints = fs
  .readFileSync(path.join(FIXTURES, 'outpoints.txt'), 'utf8')
  .trim()
  .split('\n')
  .map((line) => {
    const [txid, vout] = line.split(' ')
    return { txid, vout: Number(vout) }
  })

const transactions = {}
for (const txid of fs.readdirSync(path.join(FIXTURES, 'transactions'))) {
  transactions[txid] = fs.readFileSync(path.join(FIXTURES, 'transactions', txid), 'utf8').trim()
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

  let match
  if ((match = req.url.match(/^\/address\/([^/]+)\/utxo$/))) {
    served.utxo++
    if (match[1] !== ADDRESS) return send(200, '[]')
    return send(
      200,
      JSON.stringify(
        outpoints.map((outpoint) => ({
          ...outpoint,
          status: { confirmed: true, block_height: TIP - DEPTH_BELOW_TIP },
        })),
      ),
    )
  }

  if ((match = req.url.match(/^\/tx\/([0-9a-f]{64})\/hex$/))) {
    served.tx++
    return transactions[match[1]] ? send(200, transactions[match[1]]) : send(404, 'Not found')
  }

  send(404, 'Not found')
})

const verify = (overrides = {}) =>
  wasm.verifyProofOfReserves({
    addresses: [ADDRESS],
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
    check('the example lists one address', example.addresses.length === 1, String(example.addresses.length))
    check('the example warns its utxos are spent', /NonSpendableInput/.test(example.note), example.note.slice(0, 40) + '…')

    const proof = await verify()
    check('a valid proof verifies', proof.spendable === EXPECTED_SATS, `${proof.spendable} sats`)
    check('the network is detected', proof.network === 'testnet', proof.network)
    check('the chain tip is reported', proof.tip_height === TIP, String(proof.tip_height))
    check('every outpoint is counted', proof.utxos === outpoints.length, String(proof.utxos))
    check('transaction fetches are deduped', served.tx === 12, `${served.tx} requests for 18 utxos`)

    await rejects('a proof is bound to its message', { message: 'another message' }, 'ChallengeInputMismatch')

    // A spent UTXO has to fail the proof outright rather than quietly lowering
    // the total, which is the whole point of the exercise.
    const removed = outpoints.splice(0, 1)
    await rejects('a spent utxo fails the proof', {}, 'NonSpendableInput')
    outpoints.unshift(...removed)

    await rejects('confirmation depth is enforced', { confirmations: 200 }, 'NonSpendableInput')

    // The fixture UTXOs sit 100 blocks below the tip, so they have exactly 101
    // confirmations: the including block counts as the first. These two pin
    // that boundary down from both sides.
    const atTheLimit = await verify({ confirmations: DEPTH })
    check('the exact confirmation depth is accepted', atTheLimit.utxos === outpoints.length, `${DEPTH} confirmations`)
    await rejects('one more confirmation than exists is refused', { confirmations: DEPTH + 1 }, 'NonSpendableInput')
    await rejects('an empty address list is refused', { addresses: [] }, 'No address provided')
    await rejects('a malformed psbt is refused', { proof_psbt: 'garbage!!' }, 'Base64 decode error')
    await rejects(
      'addresses from different networks are refused',
      { addresses: ['bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq', ADDRESS] },
      'same network',
    )

    // The amounts come from the transactions rather than the UTXO listing, so a
    // server that swaps one in must be caught rather than believed.
    const [first, second] = Object.keys(transactions)
    const original = transactions[first]
    transactions[first] = transactions[second]
    await rejects('a substituted transaction is caught', {}, 'txid is')
    transactions[first] = original

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
