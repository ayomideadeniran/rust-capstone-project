import { readFileSync } from "fs";

describe('Evaluate submission', () => {
    let txid: string;
    let minerInputAddress: string;
    let minerInputAmount: number;
    let traderInputAddress: string;
    let traderInputAmount: number;
    let minerChangeAddress: string;
    let minerChangeAmount: number;
    let fee: number;
    let blockHeight: number;
    let blockHash: string;
    let tx: any;

    it('should read data from out.txt and perform sanity checks', () => {
        // read txid from out.txt
        const data = readFileSync('out.txt', 'utf8').trim().split('\n');
        expect(data.length).toBe(10);

        txid = data[0].trim();
        expect(txid).toBeDefined();
        expect(txid).toHaveLength(64);

        minerInputAddress = data[1].trim();
        expect(minerInputAddress).toBeDefined();

        minerInputAmount = parseFloat(data[2].trim());
        expect(minerInputAmount).toBeDefined();
        expect(minerInputAmount).toBeGreaterThan(0);

        traderInputAddress = data[3].trim();
        expect(traderInputAddress).toBeDefined();

        traderInputAmount = parseFloat(data[4].trim());
        expect(traderInputAmount).toBeDefined();
        expect(traderInputAmount).toBeGreaterThan(0);

        minerChangeAddress = data[5].trim();
        expect(minerChangeAddress).toBeDefined();

        minerChangeAmount = parseFloat(data[6].trim());
        expect(minerChangeAmount).toBeDefined();
        expect(minerChangeAmount).toBeGreaterThanOrEqual(0);

        fee = parseFloat(data[7].trim());
        expect(fee).toBeDefined();
        if (fee < 0) fee = -fee;
        expect(fee).toBeGreaterThan(0);

        blockHeight = parseInt(data[8].trim());
        expect(blockHeight).toBeDefined();
        expect(blockHeight).toBeGreaterThan(0);

        blockHash = data[9].trim();
        expect(blockHash).toBeDefined();
        expect(blockHash).toHaveLength(64);
    });

    it('should get transaction details from node', async () => {
        const RPC_USER = "bitcoin";
        const RPC_PASSWORD = "secret";
        const RPC_HOST = "http://127.0.0.1:18443/wallet/Miner";

        const response = await fetch(RPC_HOST, {
            method: 'post',
            body: JSON.stringify({
                jsonrpc: '1.0',
                id: 'curltest',
                method: 'gettransaction',
                params: [txid, null, true]
            }),
            headers: {
                'Content-Type': 'text/plain',
                'Authorization': 'Basic ' + Buffer.from(`${RPC_USER}:${RPC_PASSWORD}`).toString('base64'),
            }
        });
        const result = (await response.json()).result as any;
        expect(result).not.toBeNull();
        expect(result.txid).toBe(txid);

        tx = result;
    });

    it('should have the correct block height', () => {
        expect(tx.blockheight).toBe(blockHeight);
    });

    it('should have the correct block hash', () => {
        expect(tx.blockhash).toBe(blockHash);
    });

    it('should have the correct number of vins', () => {
        // The number of inputs depends on the wallet's coin selection.
        // We just need to ensure there's at least one.
        expect(tx.decoded.vin.length).toBeGreaterThanOrEqual(1);
    });

    it('should have the correct number of vouts', () => {
        // The number of outputs is 1 (to trader) + 1 (change, if any).
        const expectedVouts = minerChangeAmount > 0 ? 2 : 1;
        expect(tx.decoded.vout.length).toBe(expectedVouts);
    });

    it('should have the correct miner output', () => {
        // This test is only relevant if there was a change output.
        if (minerChangeAmount > 0) {
            const minerOutput = tx.decoded.vout.find((o: any) => o.scriptPubKey.address === minerChangeAddress);
            expect(minerOutput).toBeDefined();
            expect(minerOutput.value).toBeCloseTo(minerChangeAmount);
        }
    });

    it('should have the correct trader output', () => {
        const traderOutput = tx.decoded.vout.find((o: any) => o.scriptPubKey.address === traderInputAddress);
        expect(traderOutput).toBeDefined();
        expect(traderOutput.value).toBeCloseTo(traderInputAmount);
    });

    it('should have the correct fee', () => {
        expect(Math.abs(tx.fee)).toBeCloseTo(fee);
    });
});