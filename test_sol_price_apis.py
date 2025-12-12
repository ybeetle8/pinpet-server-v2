#!/usr/bin/env python3
"""
SOL Price API Testing Script
测试各个免费 API 的稳定性和响应速度
SOL Price API Testing Script
Tests the stability and response speed of various free APIs
"""

import requests
import time
import json
from typing import Dict, List, Tuple
from datetime import datetime

class SolPriceAPITester:
    """
    SOL 价格 API 测试器
    SOL Price API Tester
    """

    def __init__(self):
        self.apis = {
            "Binance": {
                "url": "https://api.binance.com/api/v3/ticker/price?symbol=SOLUSDT",
                "price_path": ["price"],
                "no_auth": True
            },
            "Binance 24hr": {
                "url": "https://api.binance.com/api/v3/ticker/24hr?symbol=SOLUSDT",
                "price_path": ["lastPrice"],
                "no_auth": True
            },
            "CoinGecko": {
                "url": "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd",
                "price_path": ["solana", "usd"],
                "no_auth": True
            },
            "CryptoCompare": {
                "url": "https://min-api.cryptocompare.com/data/price?fsym=SOL&tsyms=USD",
                "price_path": ["USD"],
                "no_auth": True
            },
            "Jupiter Price API": {
                "url": "https://price.jup.ag/v6/price?ids=So11111111111111111111111111111111111111112",
                "price_path": ["data", "So11111111111111111111111111111111111111112", "price"],
                "no_auth": True
            }
        }

    def test_api(self, name: str, config: Dict) -> Tuple[bool, float, str, float]:
        """
        测试单个 API / Test single API
        返回: (成功, 响应时间, 错误信息, 价格)
        Returns: (success, response_time, error_msg, price)
        """
        try:
            start_time = time.time()
            response = requests.get(config["url"], timeout=5)
            response_time = (time.time() - start_time) * 1000  # Convert to ms

            if response.status_code != 200:
                return False, response_time, f"HTTP {response.status_code}", 0

            data = response.json()
            price = data
            for key in config["price_path"]:
                price = price[key]

            price = float(price)
            return True, response_time, "OK", price

        except requests.Timeout:
            return False, 5000, "Timeout", 0
        except Exception as e:
            return False, 0, str(e), 0

    def run_tests(self, iterations: int = 5) -> Dict:
        """
        运行测试 / Run tests
        """
        results = {}

        print("=" * 80)
        print(f"SOL Price API Testing - {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
        print("=" * 80)
        print(f"\nTesting each API {iterations} times...\n")

        for name, config in self.apis.items():
            print(f"Testing {name}...")

            successes = 0
            response_times = []
            prices = []
            errors = []

            for i in range(iterations):
                success, response_time, error, price = self.test_api(name, config)

                if success:
                    successes += 1
                    response_times.append(response_time)
                    prices.append(price)
                else:
                    errors.append(error)

                # Avoid rate limiting
                time.sleep(0.5)

            results[name] = {
                "success_rate": (successes / iterations) * 100,
                "avg_response_time": sum(response_times) / len(response_times) if response_times else 0,
                "min_response_time": min(response_times) if response_times else 0,
                "max_response_time": max(response_times) if response_times else 0,
                "avg_price": sum(prices) / len(prices) if prices else 0,
                "errors": errors,
                "no_auth_required": config["no_auth"]
            }

        return results

    def print_results(self, results: Dict):
        """
        打印测试结果 / Print test results
        """
        print("\n" + "=" * 80)
        print("TEST RESULTS SUMMARY")
        print("=" * 80)

        # Sort by success rate and response time
        sorted_apis = sorted(results.items(),
                           key=lambda x: (-x[1]["success_rate"], x[1]["avg_response_time"]))

        for rank, (name, data) in enumerate(sorted_apis, 1):
            print(f"\n{rank}. {name}")
            print("-" * 40)
            print(f"  Success Rate: {data['success_rate']:.1f}%")
            if data['avg_response_time'] > 0:
                print(f"  Avg Response Time: {data['avg_response_time']:.1f}ms")
                print(f"  Min/Max Response: {data['min_response_time']:.1f}ms / {data['max_response_time']:.1f}ms")
            if data['avg_price'] > 0:
                print(f"  SOL Price: ${data['avg_price']:.2f}")
            print(f"  No Auth Required: {data['no_auth_required']}")
            if data['errors']:
                print(f"  Errors: {', '.join(set(data['errors']))}")

        print("\n" + "=" * 80)
        print("RECOMMENDATIONS")
        print("=" * 80)

        # Find best APIs
        reliable_apis = [(name, data) for name, data in results.items()
                        if data['success_rate'] >= 80]

        if reliable_apis:
            fastest = min(reliable_apis, key=lambda x: x[1]['avg_response_time'])
            print(f"\n✅ Most Stable & Fast: {fastest[0]}")
            print(f"   - {fastest[1]['success_rate']:.0f}% success rate")
            print(f"   - {fastest[1]['avg_response_time']:.0f}ms average response")

            print("\n📊 Top 3 Recommendations:")
            for i, (name, data) in enumerate(sorted_apis[:3], 1):
                if data['success_rate'] >= 80:
                    print(f"   {i}. {name} - {data['success_rate']:.0f}% stable, {data['avg_response_time']:.0f}ms")

if __name__ == "__main__":
    tester = SolPriceAPITester()
    results = tester.run_tests(iterations=5)
    tester.print_results(results)

    # Save results to JSON
    with open("sol_api_test_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print("\n💾 Results saved to sol_api_test_results.json")