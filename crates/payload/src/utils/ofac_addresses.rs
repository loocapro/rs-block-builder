use super::metrics::OfacMetrics;
use lazy_static::lazy_static;
use reth::primitives::{Address, TransactionSignedEcRecovered};
use revm_primitives::{address, HashSet};

/// Trait for checking if an address is on the OFAC blacklist
pub trait Ofac {
    /// Default method to check addresses against the OFAC list.
    /// Returns the number of addresses that got censored.
    fn check(addresses: &[&Address]) -> usize {
        addresses
            .iter()
            .filter(|&&address| OFAC_ADDRESSES.contains(address))
            .count()
    }

    /// Default method to increment the censored metric.
    /// This method should be called if `check` returns more than 0.
    fn count(n: u64) {
        let metrics = OfacMetrics::default();
        metrics.increment_censored(n);
    }

    /// Check if an address is on the OFAC blacklist
    fn contains_ofac_addresses(&self) -> bool;
}

impl Ofac for Address {
    fn contains_ofac_addresses(&self) -> bool {
        let is_censored = Self::check(&[self]) > 0;
        if is_censored {
            // we increment by 1 because we are checking a single address
            Self::count(1);
        }
        is_censored
    }
}

impl Ofac for TransactionSignedEcRecovered {
    fn contains_ofac_addresses(&self) -> bool {
        let signer = self.signer();
        let recipient = self.to();

        let signer_and_recipient: Vec<&Address> = vec![Some(&signer), recipient.as_ref()]
            .into_iter()
            .flatten()
            .collect();

        let censored_count = Self::check(&signer_and_recipient);
        let is_censored = censored_count > 0;

        if is_censored {
            // we might censor both the signer and the recipient
            Self::count(censored_count as u64);
            return is_censored;
        }
        is_censored
    }
}

lazy_static! {
    /// OFAC blacklisted addresses as of 2024-02-06
    /// Source: <https://github.com/ultrasoundmoney/ofac-ethereum-addresses/tree/main>
    pub static ref OFAC_ADDRESSES: HashSet<Address> = {
        let mut set = HashSet::new();
        set.insert(address!("8576acc5c05d6ce88f4e49bf65bdf0c62f91353c"));
        set.insert(address!("901bb9583b24d97e995513c6778dc6888ab6870e"));
        set.insert(address!("a7e5d5a720f06526557c513402f2e6b5fa20b008"));
        set.insert(address!("d882cfc20f52f2599d84b8e8d58c7fb62cfe344b"));
        set.insert(address!("7f367cc41522ce07553e823bf3be79a889debe1b"));
        set.insert(address!("1da5821544e25c636c1417ba96ade4cf6d2f9b5a"));
        set.insert(address!("7db418b5d567a4e0e8c59ad71be1fce48f3e6107"));
        set.insert(address!("72a5843cc08275c8171e582972aa4fda8c397b2a"));
        set.insert(address!("7f19720a857f834887fc9a7bc0a0fbe7fc7f8102"));
        set.insert(address!("9f4cda013e354b8fc285bf4b9a60460cee7f7ea9"));
        set.insert(address!("2f389ce8bd8ff92de3402ffce4691d17fc4f6535"));
        set.insert(address!("19aa5fe80d33a56d56c78e82ea5e50e5d80b4dff"));
        set.insert(address!("e7aa314c77f4233c18c6cc84384a9247c0cf367b"));
        set.insert(address!("308ed4b7b49797e1a98d3818bff6fe5385410370"));
        set.insert(address!("fec8a60023265364d066a1212fde3930f6ae8da7"));
        set.insert(address!("67d40EE1A85bf4a4Bb7Ffae16De985e8427B6b45"));
        set.insert(address!("6f1ca141a28907f78ebaa64fb83a9088b02a8352"));
        set.insert(address!("6acdfba02d390b97ac2b2d42a63e85293bcc160e"));
        set.insert(address!("48549a34ae37b12f6a30566245176994e17c6b4a"));
        set.insert(address!("5512d943ed1f7c8a43f3435c85f7ab68b30121b0"));
        set.insert(address!("c455f7fd3e0e12afd51fba5c106909934d8a0e4a"));
        set.insert(address!("3cbded43efdaf0fc77b9c55f6fc9988fcc9b757d"));
        set.insert(address!("7ff9cfad3877f21d41da833e2f775db0569ee3d9"));
        set.insert(address!("098b716b8aaf21512996dc57eb0615e2383e2f96"));
        set.insert(address!("a0e1c89ef1a489c9c7de96311ed5ce5d32c20e4b"));
        set.insert(address!("3cffd56b47b7b41c56258d9c7731abadc360e073"));
        set.insert(address!("53b6936513e738f44fb50d2b9476730c0ab3bfc1"));
        set.insert(address!("35fb6f6db4fb05e6a4ce86f2c93691425626d4b1"));
        set.insert(address!("f7b31119c2682c88d88d455dbb9d5932c65cf1be"));
        set.insert(address!("3e37627deaa754090fbfbb8bd226c1ce66d255e9"));
        set.insert(address!("08723392ed15743cc38513c4925f5e6be5c17243"));
        set.insert(address!("8589427373d6d84e98730d7795d8f6f8731fda16"));
        set.insert(address!("722122df12d4e14e13ac3b6895a86e84145b6967"));
        set.insert(address!("dd4c48c0b24039969fc16d1cdf626eab821d3384"));
        set.insert(address!("d90e2f925da726b50c4ed8d0fb90ad053324f31b"));
        set.insert(address!("d96f2b1c14db8458374d9aca76e26c3d18364307"));
        set.insert(address!("4736dcf1b7a3d580672cce6e7c65cd5cc9cfba9d"));
        set.insert(address!("d4b88df4d29f5cedd6857912842cff3b20c8cfa3"));
        set.insert(address!("910cbd523d972eb0a6f4cae4618ad62622b39dbf"));
        set.insert(address!("a160cdab225685da1d56aa342ad8841c3b53f291"));
        set.insert(address!("fd8610d20aa15b7b2e3be39b396a1bc3516c7144"));
        set.insert(address!("f60dd140cff0706bae9cd734ac3ae76ad9ebc32a"));
        set.insert(address!("22aaa7720ddd5388a3c0a3333430953c68f1849b"));
        set.insert(address!("ba214c1c1928a32bffe790263e38b4af9bfcd659"));
        set.insert(address!("b1c8094b234dce6e03f10a5b673c1d8c69739a00"));
        set.insert(address!("527653ea119f3e6a1f5bd18fbf4714081d7b31ce"));
        set.insert(address!("58e8dcc13be9780fc42e8723d8ead4cf46943df2"));
        set.insert(address!("d691f27f38b395864ea86cfc7253969b409c362d"));
        set.insert(address!("aeaac358560e11f52454d997aaff2c5731b6f8a6"));
        set.insert(address!("1356c899d8c9467c7f71c195612f8a395abf2f0a"));
        set.insert(address!("a60c772958a3ed56c1f15dd055ba37ac8e523a0d"));
        set.insert(address!("169ad27a470d064dede56a2d3ff727986b15d52b"));
        set.insert(address!("0836222f2b2b24a3f36f98668ed8f0b38d1a872f"));
        set.insert(address!("f67721a2d8f736e75a49fdd7fad2e31d8676542a"));
        set.insert(address!("9ad122c22b14202b4490edaf288fdb3c7cb3ff5e"));
        set.insert(address!("905b63fff465b9ffbf41dea908ceb12478ec7601"));
        set.insert(address!("07687e702b410fa43f4cb4af7fa097918ffd2730"));
        set.insert(address!("94a1b5cdb22c43faab4abeb5c74999895464ddaf"));
        set.insert(address!("b541fc07bc7619fd4062a54d96268525cbc6ffef"));
        set.insert(address!("12d66f87a04a9e220743712ce6d9bb1b5616b8fc"));
        set.insert(address!("47ce0c6ed5b0ce3d3a51fdb1c52dc66a7c3c2936"));
        set.insert(address!("23773e65ed146a459791799d01336db287f25334"));
        set.insert(address!("d21be7248e0197ee08e0c20d4a96debdac3d20af"));
        set.insert(address!("610b717796ad172b316836ac95a2ffad065ceab4"));
        set.insert(address!("178169b423a011fff22b9e3f3abea13414ddd0f1"));
        set.insert(address!("bb93e510bbcd0b7beb5a853875f9ec60275cf498"));
        set.insert(address!("2717c5e28cf931547b621a5dddb772ab6a35b701"));
        set.insert(address!("03893a7c7463ae47d46bc7f091665f1893656003"));
        set.insert(address!("ca0840578f57fe71599d29375e16783424023357"));
        set.insert(address!("c2a3829f459b3edd87791c74cd45402ba0a20be3"));
        set.insert(address!("3ad9db589d201a710ed237c829c7860ba86510fc"));
        set.insert(address!("3aac1cc67c2ec5db4ea850957b967ba153ad6279"));
        set.insert(address!("76d85b4c0fc497eecc38902397ac608000a06607"));
        set.insert(address!("0e3a09dda6b20afbb34ac7cd4a6881493f3e7bf7"));
        set.insert(address!("723b78e67497e85279cb204544566f4dc5d2aca0"));
        set.insert(address!("cc84179ffd19a1627e79f8648d09e095252bc418"));
        set.insert(address!("6bf694a291df3fec1f7e69701e3ab6c592435ae7"));
        set.insert(address!("330bdfade01ee9bf63c209ee33102dd334618e0a"));
        set.insert(address!("a5c2254e4253490c54cef0a4347fddb8f75a4998"));
        set.insert(address!("af4c0b70b2ea9fb7487c7cbb37ada259579fe040"));
        set.insert(address!("df231d99ff8b6c6cbf4e9b9a945cbacef9339178"));
        set.insert(address!("1e34a77868e19a6647b1f2f47b51ed72dede95dd"));
        set.insert(address!("d47438c816c9e7f2e2888e060936a499af9582b3"));
        set.insert(address!("84443cfd09a48af6ef360c6976c5392ac5023a1f"));
        set.insert(address!("d5d6f8d9e784d0e26222ad3834500801a68d027d"));
        set.insert(address!("af8d1839c3c67cf571aa74b5c12398d4901147b3"));
        set.insert(address!("407cceeaa7c95d2fe2250bf9f2c105aa7aafb512"));
        set.insert(address!("05e0b5b40b7b66098c2161a5ee11c5740a3a7c45"));
        set.insert(address!("d8d7de3349ccaa0fde6298fe6d7b7d0d34586193"));
        set.insert(address!("3efa30704d2b8bbac821307230376556cf8cc39e"));
        set.insert(address!("746aebc06d2ae31b71ac51429a19d54e797878e9"));
        set.insert(address!("5f6c97c6ad7bdd0ae7e0dd4ca33a4ed3fdabd4d7"));
        set.insert(address!("f4b067dd14e95bab89be928c07cb22e3c94e0daa"));
        set.insert(address!("01e2919679362dfbc9ee1644ba9c6da6d6245bb1"));
        set.insert(address!("2fc93484614a34f26f7970cbb94615ba109bb4bf"));
        set.insert(address!("26903a5a198d571422b2b4ea08b56a37cbd68c89"));
        set.insert(address!("b20c66c4de72433f3ce747b58b86830c459ca911"));
        set.insert(address!("2573bac39ebe2901b4389cd468f2872cf7767faf"));
        set.insert(address!("653477c392c16b0765603074f157314cc4f40c32"));
        set.insert(address!("88fd245fedec4a936e700f9173454d1931b4c307"));
        set.insert(address!("09193888b3f38c82dedfda55259a82c0e7de875e"));
        set.insert(address!("5cab7692d4e94096462119ab7bf57319726eed2a"));
        set.insert(address!("756c4628e57f7e7f8a459ec2752968360cf4d1aa"));
        set.insert(address!("d82ed8786d7c69dc7e052f7a542ab047971e73d2"));
        set.insert(address!("77777feddddffc19ff86db637967013e6c6a116c"));
        set.insert(address!("833481186f16cece3f1eeea1a694c42034c3a0db"));
        set.insert(address!("b04e030140b30c27bcdfaafffa98c57d80eda7b4"));
        set.insert(address!("cee71753c9820f063b38fdbe4cfdaf1d3d928a80"));
        set.insert(address!("8281aa6795ade17c8973e1aedca380258bc124f9"));
        set.insert(address!("57b2b8c82f065de8ef5573f9730fc1449b403c9f"));
        set.insert(address!("23173fe8b96a4ad8d2e17fb83ea5dcccdca1ae52"));
        set.insert(address!("538ab61e8a9fc1b2f93b3dd9011d662d89be6fe6"));
        set.insert(address!("94be88213a387e992dd87de56950a9aef34b9448"));
        set.insert(address!("242654336ca2205714071898f67e254eb49acdce"));
        set.insert(address!("776198ccf446dfa168347089d7338879273172cf"));
        set.insert(address!("edc5d01286f99a066559f60a585406f3878a033e"));
        set.insert(address!("d692fd2d0b2fbd2e52cfa5b5b9424bc981c30696"));
        set.insert(address!("df3a408c53e5078af6e8fb2a85088d46ee09a61b"));
        set.insert(address!("743494b60097a2230018079c02fe21a7b687eaa5"));
        set.insert(address!("94c92f096437ab9958fc0a37f09348f30389ae79"));
        set.insert(address!("5efda50f22d34f262c29268506c5fa42cb56a1ce"));
        set.insert(address!("2f50508a8a3d323b91336fa3ea6ae50e55f32185"));
        set.insert(address!("179f48c78f57a3a78f0608cc9197b8972921d1d2"));
        set.insert(address!("ffbac21a641dcfe4552920138d90f3638b3c9fba"));
        set.insert(address!("d0975b32cea532eadddfc9c60481976e39db3472"));
        set.insert(address!("1967d8af5bd86a497fb3dd7899a020e47560daaf"));
        set.insert(address!("83e5bc4ffa856bb84bb88581f5dd62a433a25e0d"));
        set.insert(address!("08b2eFdcdB8822EfE5ad0Eae55517cf5DC544251"));
        set.insert(address!("04DBA1194ee10112fE6C3207C0687DEf0e78baCf"));
        set.insert(address!("0Ee5067b06776A89CcC7dC8Ee369984AD7Db5e06"));
        set.insert(address!("502371699497d08D5339c870851898D6D72521Dd"));
        set.insert(address!("5A14E72060c11313E38738009254a90968F58f51"));
        set.insert(address!("EFE301d259F525cA1ba74A7977b80D5b060B3ccA"));
        set.insert(address!("39d908dac893cbcb53cc86e0ecc369aa4def1a29"));
        set.insert(address!("4f47bc496083c727c5fbe3ce9cdf2b0f6496270c"));
        set.insert(address!("38735f03b30FbC022DdD06ABED01F0Ca823C6a94"));
        set.insert(address!("97b1043abd9e6fc31681635166d430a458d14f9c"));
        set.insert(address!("b6f5ec1a0a9cd1526536d3f0426c429529471f40"));
        set.insert(address!("dcbEfFBECcE100cCE9E4b153C4e15cB885643193"));
        set.insert(address!("5f48c2a71b2cc96e3f0ccae4e39318ff0dc375b2"));
        set.insert(address!("5a7a51bfb49f190e5a6060a5bc6052ac14a3b59f"));
        set.insert(address!("ed6e0a7e4ac94d976eebfb82ccf777a3c6bad921"));
        set.insert(address!("797d7ae72ebddcdea2a346c1834e04d1f8df102b"));
        set.insert(address!("931546D9e66836AbF687d2bc64B30407bAc8C568"));
        set.insert(address!("43fa21d92141BA9db43052492E0DeEE5aa5f0A93"));
        set.insert(address!("6be0ae71e6c41f2f9d0d1a3b8d0f75e6f6a0b46e"));
        set.insert(address!("9c2bc757b66f24d60f016b6237f8cdd414a879fa"));
        set.insert(address!("530a64c0ce595026a4a556b703644228179e2d57"));
        set.insert(address!("fac583c0cf07ea434052c49115a4682172ab6b4f"));
        set.insert(address!("961c5be54a2ffc17cf4cb021d863c42dacd47fc1"));
        set.insert(address!("983a81ca6fb1e441266d2fbcb7d8e530ac2e05a2"));
        set
    };


}
